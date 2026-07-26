//! User exit payouts: drop every epoch-eligible [`PendingUnstakeTranche`], pay their combined amount
//! from [`Validator::pending_to_claim`], then transfer to the user.
//!
//! **Flow (WASM):** `withdraw` → shared per-epoch settlement → [`Contract::payout_user_withdraw`] (claim bucket + transfer).
//!
//! **Non-WASM:** `testing_env!` skips promise chains; [`Contract::payout_user_withdraw`] runs directly from [`Contract::withdraw`].

use crate::utils::epoch_height;
use crate::*;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken, Promise, assert_one_yocto, env, near, require};

// Public `withdraw` + private promise callback (WASM continuation).

#[near]
impl Contract {
    /// Claim NEAR from `pending_to_claim` for this account on `validator_id` (epoch-eligible tranches)
    /// and **transfer it immediately** to the caller.
    ///
    /// **WASM:** Uses [`crate::epoch::Contract::promise_validator_per_epoch_settlement_then`] so balance
    /// refresh, withdraw-if-ready, and net settle run before the payout (same ordering as **`lock`** /
    /// **`unlock`**).
    ///
    /// **Non-WASM:** Runs [`Contract::payout_user_withdraw`] only (host `testing_env!` does not execute pool promise chains;
    /// see `lock.rs`).
    #[payable]
    pub fn withdraw(&mut self, validator_id: ValidatorId) -> Promise {
        assert_one_yocto();
        self.require_enough_gas_for_epoch_settlement();
        self.assert_not_paused();

        let account_id = env::predecessor_account_id();
        self.ensure_min_base_storage(&account_id);

        let account_validator_key = (account_id.clone(), validator_id.clone());
        // Must have tranches from a prior `internal_unstake` / unlock on this pool.
        let user_pending_tranches_yocto =
            self.sum_user_unstake_tranches_yocto(&account_validator_key);
        require!(
            user_pending_tranches_yocto > 0,
            "You have no unlocked NEAR waiting to claim for this validator"
        );

        self.require_validator_idle(&validator_id);

        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.payout_user_withdraw(account_id, validator_id);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.promise_validator_per_epoch_settlement_then(
                validator_id.clone(),
                UserAction::WithdrawUserTransfer {
                    validator_id,
                    account_id,
                },
            )
        }
    }

    /// **[Pipeline 5c]** User transfer tail after shared pre-user settlement (**0–4**).
    #[private]
    pub fn on_withdraw_user_transfer_after_settle(
        &mut self,
        account_id: AccountId,
        validator_id: ValidatorId,
    ) -> Promise {
        self.payout_user_withdraw(account_id, validator_id)
    }

    /// Return UI-ready pending unstake and withdraw status for one account on one validator.
    pub fn get_account_pending_unstake(
        &self,
        account_id: AccountId,
        validator_id: ValidatorId,
    ) -> AccountPendingUnstakeView {
        let validator = self.require_validator(&validator_id);
        let current_epoch_height = epoch_height();
        let tranches = self
            .user_pending_unstake
            .get(&(account_id.clone(), validator_id.clone()))
            .cloned()
            .unwrap_or_default();

        let mut total_pending_yocto = 0u128;
        let mut epoch_eligible_yocto = 0u128;
        let mut next_available_epoch_height = None;
        let tranches = tranches
            .into_iter()
            .map(|tranche| {
                let amount_yocto = tranche.amount.as_yoctonear();
                let is_epoch_eligible = current_epoch_height >= tranche.available_epoch_height;
                total_pending_yocto = total_pending_yocto.saturating_add(amount_yocto);
                if is_epoch_eligible {
                    epoch_eligible_yocto = epoch_eligible_yocto.saturating_add(amount_yocto);
                } else {
                    next_available_epoch_height = Some(
                        next_available_epoch_height
                            .map_or(tranche.available_epoch_height, |next: u64| {
                                next.min(tranche.available_epoch_height)
                            }),
                    );
                }
                PendingUnstakeTrancheView {
                    amount_yocto: U128(amount_yocto),
                    available_epoch_height: tranche.available_epoch_height,
                    is_epoch_eligible,
                }
            })
            .collect();

        let pending_to_claim_yocto = validator.pending_to_claim.as_yoctonear();
        let can_withdraw_now = !self.paused
            && validator.tx_status == TransactionStatus::Idle
            && epoch_eligible_yocto > 0
            && pending_to_claim_yocto >= epoch_eligible_yocto;
        let withdrawable_yocto = if can_withdraw_now {
            epoch_eligible_yocto
        } else {
            0
        };
        let wait_epochs = next_available_epoch_height
            .map(|available_epoch| available_epoch.saturating_sub(current_epoch_height));

        AccountPendingUnstakeView {
            account_id,
            validator_id,
            current_epoch_height,
            epoch_unstake_settle_epochs: self.internal_get_config().epoch_unstake_settle_epochs,
            total_pending_yocto: U128(total_pending_yocto),
            epoch_eligible_yocto: U128(epoch_eligible_yocto),
            withdrawable_yocto: U128(withdrawable_yocto),
            next_available_epoch_height,
            wait_epochs,
            pending_to_claim_yocto: U128(pending_to_claim_yocto),
            can_withdraw_now,
            tranches,
        }
    }
}

// Tranche math, bucket claim (no transfer), and payout orchestration.

impl Contract {
    /// Total yocto across **all** tranches for `(account, validator)` (includes not-yet-claimable epochs).
    fn sum_user_unstake_tranches_yocto(
        &self,
        account_validator_key: &(AccountId, ValidatorId),
    ) -> u128 {
        self.user_pending_unstake
            .get(account_validator_key)
            .map(|tranches| {
                tranches
                    .iter()
                    .map(|tranche| tranche.amount.as_yoctonear())
                    .fold(0u128, |sum, yocto| sum.saturating_add(yocto))
            })
            .unwrap_or(0)
    }

    /// Removes every tranche with `available_epoch_height <= at_epoch` and returns their total yocto.
    /// Non-claimable tranches are kept. Returns `true` when no tranches remain.
    fn remove_claimable_tranches(
        &mut self,
        account_validator_key: &(AccountId, ValidatorId),
        at_epoch: u64,
    ) -> (u128, bool) {
        let mut tranches = self
            .user_pending_unstake
            .get(account_validator_key)
            .cloned()
            .unwrap_or_default();
        let claimable_yocto = tranches
            .iter()
            .filter(|tranche| at_epoch >= tranche.available_epoch_height)
            .map(|tranche| tranche.amount.as_yoctonear())
            .fold(0u128, |sum, yocto| sum.saturating_add(yocto));
        tranches.retain(|tranche| at_epoch < tranche.available_epoch_height);
        let user_done = tranches.is_empty();
        self.set_user_pending_unstake_tranches(account_validator_key.clone(), tranches);
        (claimable_yocto, user_done)
    }

    /// Drops all epoch-eligible tranches, debits the claim bucket by their sum, and returns that NEAR.
    /// Use [`Contract::payout_user_withdraw`] to attach the transfer promise.
    pub(crate) fn claim_from_withdraw_bucket(
        &mut self,
        account_id: AccountId,
        validator_id: ValidatorId,
    ) -> NearToken {
        let account_validator_key = (account_id.clone(), validator_id.clone());
        let mut validator = self.require_validator(&validator_id);
        let pending_claim_bucket_yocto = validator.pending_to_claim.as_yoctonear();
        require!(
            pending_claim_bucket_yocto > 0,
            "No NEAR is claimable yet; wait until unstaked funds are withdrawn from the pool into this contract, then retry"
        );
        let eh = epoch_height();
        let (credit_yocto, _) = self.remove_claimable_tranches(&account_validator_key, eh);
        require!(
            credit_yocto > 0,
            "Nothing to claim yet: wait until `epoch_height >=` your tranche's available epoch height"
        );
        require!(
            pending_claim_bucket_yocto >= credit_yocto,
            "Claim bucket cannot cover all claimable tranches yet; call withdraw again after more pool funds arrive"
        );

        validator.pending_to_claim = validator
            .pending_to_claim
            .checked_sub(NearToken::from_yoctonear(credit_yocto))
            .expect("pending_to_claim accounting mismatch; contact the contract maintainers");
        let credit = NearToken::from_yoctonear(credit_yocto);

        self.internal_set_validator(validator_id.clone(), validator);

        crate::events::log_withdraw(&account_id, &validator_id, credit.as_yoctonear());
        credit
    }

    /// Claim from `pending_to_claim` and transfer to the user. Pool → contract withdraw runs in the
    /// shared per-epoch settlement chain before this ([`crate::epoch::Contract::promise_validator_per_epoch_settlement_then`]).
    ///
    /// Called from [`Contract::withdraw`] (non-WASM / tests) and from [`crate::epoch::Contract::on_epoch_settlement_dispatch_continue`] on WASM.
    pub(crate) fn payout_user_withdraw(
        &mut self,
        account_id: AccountId,
        validator_id: ValidatorId,
    ) -> Promise {
        let credit = self.claim_from_withdraw_bucket(account_id.clone(), validator_id);
        require!(
            env::account_balance().as_yoctonear() >= credit.as_yoctonear(),
            "Contract does not hold enough NEAR to complete this withdraw transfer yet; retry after pool funds arrive"
        );
        Promise::new(account_id).transfer(credit)
    }
}
