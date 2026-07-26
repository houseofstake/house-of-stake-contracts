//! Validator pool pipeline: balance refresh, withdraw-if-ready, net stake/unstake (one mutating pool op per NEAR epoch per pool).
//! Pool pipeline design: [`docs/features/lazy-epoch-pipeline.md`](../docs/features/lazy-epoch-pipeline.md).
//!
//! **Entry:** [`Contract::promise_validator_per_epoch_settlement_then`] (**0**, sets **`Busy`**) — shared by `lock`, `unlock`, `withdraw`, `epoch_settle`.
//! Fast path when [`Validator::last_settlement_epoch`] ≥ current epoch: **0** → **4** only.
//! A successful fresh-epoch settlement decision advances [`Validator::last_settlement_epoch`],
//! even when no stake/unstake work is pending or unstake work is still in its wait window.
//! User tails **5a–5c** live in `lock.rs`, `unlock.rs`, `withdraw.rs`; **6** clears **`Idle`**.

use crate::events;
use crate::gas::{callbacks, staking_pool};
use crate::utils::{block_timestamp, epoch_height};
use crate::*;
use near_sdk::ext_contract;
use near_sdk::json_types::{U64, U128};
use near_sdk::{
    AccountId, NearToken, Promise, PromiseError, PromiseOrValue, env, is_promise_success, near,
    require,
};

// External interfaces (cross-contract), ordered by pipeline step.

/// Self callbacks for epoch / pool work. Names are **stable promise targets** (`#[private]`); rename only
/// with a coordinated ABI / off-chain migration.
#[ext_contract(ext_self_epoch)]
pub trait ExtSelfEpoch {
    /// **[Pipeline 1]** After pool `get_account`: refresh balance, optional **2a–2c**, then **3**.
    fn on_epoch_settlement_after_pool_account(
        &mut self,
        #[callback_result] pool_account_result: Result<PoolAccountView, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> Promise;
    /// **[Pipeline 2b]** Moves `pending_to_withdraw` -> `pending_to_claim` after pool `withdraw` (stays **`Busy`**).
    fn on_epoch_withdraw_transfer_done(
        &mut self,
        validator_id: ValidatorId,
        withdrawn: NearToken,
    ) -> PromiseOrValue<bool>;
    /// **[Pipeline 2c]** After pool `withdraw`: continue through settlement **3** + **4**.
    fn on_after_pool_withdraw_maybe_settle(
        &mut self,
        #[callback_result] withdraw_result: Result<bool, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<bool>;
    /// **[Pipeline 3b]** Pool `deposit_and_stake` result: updates pending queues, bumps `last_settlement_epoch`.
    fn on_deposit_and_stake(&mut self, validator_id: ValidatorId) -> bool;
    /// **[Pipeline 3c]** Pool `unstake` result: records `last_unstake_epoch`, adjusts pending queues.
    fn on_unstake(&mut self, validator_id: ValidatorId) -> bool;
    /// **[Pipeline 4]** Fan-out to user tail (**5a** / **5b** / **5c**), then **6**.
    fn on_epoch_settlement_dispatch_continue(&mut self, cont: UserAction) -> Promise;
    /// **[Pipeline 5a]** Catalog mint; may re-enter **3** (`lock.rs`).
    fn resolve_lock(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        duration_ns: u128,
        order: OrderRef,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<LockId>;
    /// **[Pipeline 5a]** Recurring subscription lock resolved after settlement.
    fn resolve_recurring_subscription_lock_after_settle(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<LockId>;
    /// **[Pipeline 5e]** Farm stake after pre-user settlement (`stake.rs`).
    fn resolve_farm_stake(
        &mut self,
        account_id: AccountId,
        deposit: NearToken,
        product_id: ProductId,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> FarmPosition;
    /// **[Pipeline 5d]** Subscription update after pre-user settlement (`subscriptions.rs`).
    fn on_subscription_update_after_settle(
        &mut self,
        buyer: AccountId,
        deposit: NearToken,
        target_price_id: PriceId,
        target_amount: U128,
        subscription_id: SubscriptionId,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<SubscriptionPlanChangeOutcome>;
    /// **[Pipeline 5b]** Share exit after pre-user settlement (`unlock.rs`).
    fn resolve_unlock(
        &mut self,
        lock_id: LockId,
        account_id: AccountId,
        validator_id: ValidatorId,
        shares_remove: u128,
    );
    /// **[Pipeline 5f]** Farm share exit after pre-user settlement (`stake.rs`).
    fn resolve_farm_unstake(
        &mut self,
        account_id: AccountId,
        product_id: ProductId,
        validator_id: ValidatorId,
        amount: Option<U128>,
    );
    /// **[Pipeline 5c]** User withdraw transfer after pre-user settlement (`withdraw.rs`).
    fn on_withdraw_user_transfer_after_settle(
        &mut self,
        account_id: AccountId,
        validator_id: ValidatorId,
    ) -> Promise;
    /// **[Pipeline 6]** After **4** tail promise completes: sets pipeline **`Idle`**.
    fn on_epoch_pipeline_terminal_release(&mut self, validator_id: ValidatorId);
    /// **[Pipeline 6]** Release **`Busy`** and return lock id from mint tail; refund on tail failure.
    fn on_epoch_pipeline_release_with_lock_id(
        &mut self,
        #[callback_result] lock_id_result: Result<LockId, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<LockId>;
    /// **[Pipeline 6]** Release **`Busy`** and return farm position from stake tail; refund on tail failure.
    fn on_epoch_pipeline_release_with_farm_position(
        &mut self,
        #[callback_result] position_result: Result<FarmPosition, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<FarmPosition>;
    /// **[Pipeline 6]** Release **`Busy`** and return subscription update outcome; refund on tail failure.
    fn on_epoch_pipeline_release_with_subscription_update_outcome(
        &mut self,
        #[callback_result] outcome_result: Result<SubscriptionPlanChangeOutcome, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<SubscriptionPlanChangeOutcome>;
}

#[ext_contract(ext_staking_pool)]
pub trait ExtStakingPool {
    /// Staking-pool view: on-chain source of truth for who may operate the pool off this contract.
    fn get_owner_id(&self) -> AccountId;
    fn deposit_and_stake(&mut self);
    fn unstake(&mut self, amount: NearToken);
    /// Move unstaked NEAR from the pool to `account_id` (this contract).
    fn withdraw(&mut self, amount: NearToken);
    /// Staked + unstaked balances and pool withdraw flag (NEAR core staking-pool `get_account`).
    fn get_account(&self, account_id: AccountId) -> PoolAccountView;
}

// Public epoch API.

#[near]
impl Contract {
    /// Public entry → **[Pipeline 0]** with [`UserAction::SettleOnly`] (tail **5** = no-op, then **6**).
    pub fn epoch_settle(&mut self, validator_id: ValidatorId) -> Promise {
        self.assert_not_paused();
        self.require_enough_gas_for_epoch_settlement();
        self.promise_validator_per_epoch_settlement_then(
            validator_id.clone(),
            UserAction::SettleOnly { validator_id },
        )
    }
}

// Epoch pipeline (`#[near]` impl, ordered by pipeline step; see module docs).

#[near]
impl Contract {
    // --- [Pipeline 0] ---

    /// **[Pipeline 0]** Entry: set **`Busy`**, then **1–3** (full) or **4** (fast path). See module doc step map.
    pub(crate) fn promise_validator_per_epoch_settlement_then(
        &mut self,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> Promise {
        let mut validator = self.require_validator_idle(&validator_id);
        // Fast path when this pool already consumed its one settle slot for the current NEAR epoch.
        let needs_pre_user_settlement_pipeline = validator.last_settlement_epoch < epoch_height();
        validator.tx_status = TransactionStatus::Busy;
        self.internal_set_validator(validator_id.clone(), validator);

        if !needs_pre_user_settlement_pipeline {
            // Same-contract fast-path dispatch: no cross-contract boundary is needed here.
            return self.on_epoch_settlement_dispatch_continue(cont);
        }
        self.promise_get_validator_pool_account(validator_id.clone())
            .then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_SETTLEMENT_AFTER_POOL_ACCOUNT)
                    .on_epoch_settlement_after_pool_account(validator_id, cont),
            )
    }

    /// Pool `get_account` before **[Pipeline 1]**.
    pub(crate) fn promise_get_validator_pool_account(&self, validator_id: ValidatorId) -> Promise {
        ext_staking_pool::ext(validator_id)
            .with_static_gas(staking_pool::GET_ACCOUNT)
            .get_account(env::current_account_id())
    }

    // --- [Pipeline 1] ---

    /// **[Pipeline 1]** After pool `get_account`: refresh balance, optional **2a–2c**, then **3**.
    #[private]
    pub fn on_epoch_settlement_after_pool_account(
        &mut self,
        #[callback_result] pool_account_result: Result<PoolAccountView, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> Promise {
        let pool_account = match pool_account_result {
            Ok(pool_account) => pool_account,
            Err(_) => {
                if let Some((buyer, amount)) = cont.payable_refund() {
                    return self.refund_payable_pipeline(&validator_id, buyer, amount);
                }
                events::log_epoch_operation("epoch_get_account_failed_release", &validator_id);
                return ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_PIPELINE_TERMINAL_RELEASE)
                    .on_epoch_pipeline_terminal_release(validator_id);
            }
        };
        let mut validator = self.require_validator(&validator_id);
        self.assert_validator_busy(
            &validator,
            "Validator pool must be busy for settlement after get_account",
        );
        validator.total_staked_balance = pool_account.total_balance();
        validator.last_balance_refresh_ns = U64(block_timestamp());
        self.internal_set_validator(validator_id.clone(), validator);

        let unstaked = pool_account.unstaked();

        if unstaked.as_yoctonear() > 0 && pool_account.can_withdraw {
            return self
                .try_epoch_withdraw_known_unstaked(
                    validator_id.clone(),
                    unstaked,
                    pool_account.can_withdraw,
                )
                .then(
                    ext_self_epoch::ext(env::current_account_id())
                        .with_static_gas(callbacks::ON_GET_UNSTAKED_FOR_WITHDRAW)
                        .on_after_pool_withdraw_maybe_settle(validator_id, cont),
                );
        }

        self.try_epoch_stake_or_unstake(validator_id, cont)
    }

    // --- [Pipeline 2a] ---

    /// **[Pipeline 2a]** Pull unstaked NEAR from pool into this contract (from **1**).
    pub(crate) fn try_epoch_withdraw_known_unstaked(
        &mut self,
        validator_id: ValidatorId,
        unstaked_balance: NearToken,
        pool_can_withdraw: bool,
    ) -> Promise {
        require!(
            unstaked_balance.as_yoctonear() > 0,
            "Withdraw requires positive unstaked balance from the pool"
        );
        require!(
            pool_can_withdraw,
            "Pool reports unstaked balance is not yet withdrawable"
        );

        events::log_epoch_operation("epoch_withdraw", &validator_id);

        let validator = self.require_validator(&validator_id);
        self.assert_validator_busy(
            &validator,
            "Validator pool must be busy for this withdraw step",
        );

        self.promise_epoch_withdraw_unstaked(validator_id, unstaked_balance)
    }

    /// **[Pipeline 2a]** Pool `withdraw` promise → callback **2b**.
    pub(crate) fn promise_epoch_withdraw_unstaked(
        &self,
        validator_id: ValidatorId,
        unstaked_balance: NearToken,
    ) -> Promise {
        ext_staking_pool::ext(validator_id.clone())
            .with_static_gas(staking_pool::WITHDRAW)
            .withdraw(unstaked_balance)
            .then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_WITHDRAW_TRANSFER)
                    .on_epoch_withdraw_transfer_done(validator_id, unstaked_balance),
            )
    }

    // --- [Pipeline 2b] ---

    /// **[Pipeline 2b]** After pool `withdraw`: credit [`Validator::pending_to_claim`] (stays **`Busy`**).
    #[private]
    pub fn on_epoch_withdraw_transfer_done(
        &mut self,
        validator_id: ValidatorId,
        withdrawn: NearToken,
    ) -> PromiseOrValue<bool> {
        let ok = is_promise_success();
        let mut validator = self.require_validator(&validator_id);
        let credited_yocto = if ok { withdrawn.as_yoctonear() } else { 0 };
        if ok && credited_yocto > 0 {
            let add = NearToken::from_yoctonear(credited_yocto);
            let pending_withdraw_yocto = validator.pending_to_withdraw.as_yoctonear();
            let bal_y = validator.total_staked_balance.as_yoctonear();
            require!(
                bal_y >= credited_yocto,
                "Recorded pool balance is less than the withdrawn amount; retry after the next successful balance refresh from the pool"
            );
            validator.total_staked_balance = NearToken::from_yoctonear(bal_y - credited_yocto);
            validator.pending_to_withdraw =
                NearToken::from_yoctonear(pending_withdraw_yocto.saturating_sub(credited_yocto));
            if pending_withdraw_yocto < credited_yocto {
                env::log_str(
                    "on_epoch_withdraw_transfer_done: pending_to_withdraw lower than withdrawn; clamped to zero",
                );
            }
            validator.pending_to_claim = validator
                .pending_to_claim
                .checked_add(add)
                .expect("pending_to_claim overflow after pool transfer");
            events::log_validator_withdraw_in(credited_yocto, &validator_id);
        }
        self.internal_set_validator(validator_id, validator);
        PromiseOrValue::Value(ok)
    }

    // --- [Pipeline 2c] ---

    /// **[Pipeline 2c]** After **2b**: continue through settlement **3** → **4**.
    #[private]
    pub fn on_after_pool_withdraw_maybe_settle(
        &mut self,
        #[callback_result] withdraw_result: Result<bool, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<bool> {
        let validator = self.require_validator(&validator_id);
        self.assert_validator_busy(
            &validator,
            "Validator pool must be busy for post-withdraw settle",
        );
        if !matches!(withdraw_result, Ok(true)) {
            events::log_epoch_operation("epoch_withdraw_failed_skip_settle", &validator_id);
            return self.on_epoch_settlement_dispatch_continue(cont).into();
        }

        self.try_epoch_stake_or_unstake(validator_id, cont).into()
    }

    // --- [Pipeline 3 / 3a] ---

    /// **[Pipeline 3]** At most one pool `deposit_and_stake` or `unstake` per NEAR epoch (**3a** net-zero inline).
    /// Skip to **4** when nothing pending or slot used; fresh-epoch no-pending and unstake-waiting
    /// decisions still mark the current epoch handled. Otherwise pool op → **3′** → **4**.
    pub(crate) fn try_epoch_stake_or_unstake(
        &mut self,
        validator_id: ValidatorId,
        dispatch_after: UserAction,
    ) -> Promise {
        let validator = self.require_validator(&validator_id);
        self.assert_validator_busy(
            &validator,
            "Validator pool must be busy for this settle step",
        );

        let pending_stake_yocto = validator.pending_to_stake.as_yoctonear();
        let pending_unstake_yocto = validator.pending_to_unstake.as_yoctonear();
        let has_pending = pending_stake_yocto > 0 || pending_unstake_yocto > 0;
        let can_settle = validator.last_settlement_epoch < epoch_height();

        if !has_pending || !can_settle {
            if !has_pending && can_settle {
                self.mark_epoch_settlement_handled(&validator_id, "epoch_settle_noop");
            }
            return self.on_epoch_settlement_dispatch_continue(dispatch_after);
        }

        if pending_stake_yocto == pending_unstake_yocto && pending_stake_yocto > 0 {
            events::log_epoch_operation("epoch_settle_net_zero", &validator_id);
            let _ = self.apply_net_zero_pending_matched_clear(&validator_id, pending_stake_yocto);
            return self.on_epoch_settlement_dispatch_continue(dispatch_after);
        }

        let pool_settle = if pending_stake_yocto > pending_unstake_yocto {
            let net = NearToken::from_yoctonear(pending_stake_yocto - pending_unstake_yocto);
            events::log_epoch_operation("epoch_settle_stake", &validator_id);
            ext_staking_pool::ext(validator_id.clone())
                .with_static_gas(staking_pool::DEPOSIT_AND_STAKE)
                .with_attached_deposit(net)
                .deposit_and_stake()
                .then(
                    ext_self_epoch::ext(env::current_account_id())
                        .with_static_gas(callbacks::ON_DEPOSIT_AND_STAKE)
                        .on_deposit_and_stake(validator_id.clone()),
                )
        } else {
            if validator.last_unstake_epoch > 0 {
                if !self.validator_unstake_waiting_finished(&validator) {
                    self.mark_epoch_settlement_handled(
                        &validator_id,
                        "epoch_settle_unstake_waiting",
                    );
                    return self.on_epoch_settlement_dispatch_continue(dispatch_after);
                }
            }
            let net = NearToken::from_yoctonear(pending_unstake_yocto - pending_stake_yocto);
            events::log_epoch_operation("epoch_settle_unstake", &validator_id);
            ext_staking_pool::ext(validator_id.clone())
                .with_static_gas(staking_pool::UNSTAKE)
                .unstake(net)
                .then(
                    ext_self_epoch::ext(env::current_account_id())
                        .with_static_gas(callbacks::ON_UNSTAKE)
                        .on_unstake(validator_id.clone()),
                )
        };

        pool_settle.then(
            ext_self_epoch::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_EPOCH_SETTLEMENT_DISPATCH)
                .on_epoch_settlement_dispatch_continue(dispatch_after),
        )
    }

    // --- [Pipeline 3a] ---

    /// Settlement reached a fresh-epoch decision without making a pool stake/unstake call. Mark the
    /// epoch handled so later user tails wait for the next epoch instead of re-opening this one.
    pub(crate) fn mark_epoch_settlement_handled(
        &mut self,
        validator_id: &ValidatorId,
        epoch_action: &str,
    ) {
        let mut validator = self.require_validator(validator_id);
        validator.last_settlement_epoch = epoch_height();
        self.internal_set_validator(validator_id.clone(), validator);
        events::log_epoch_operation(epoch_action, validator_id);
    }

    /// **[Pipeline 3a]** Inline net-zero clear (no pool `deposit` / `unstake`).
    ///
    /// When `pending_to_stake == pending_to_unstake`, we can net internally:
    /// - clear `pending_to_stake`;
    /// - move matched amount directly into `pending_to_claim` (funded by local pending stake)
    ///   without round-tripping through pool `unstake`/`withdraw`;
    /// - clear the matched `pending_to_unstake` amount without pool round-trip.
    pub(crate) fn apply_net_zero_pending_matched_clear(
        &mut self,
        validator_id: &ValidatorId,
        matched_pending_yocto: u128,
    ) -> bool {
        let mut validator = self.require_validator(validator_id);
        if validator.pending_to_stake.as_yoctonear() != matched_pending_yocto
            || validator.pending_to_unstake.as_yoctonear() != matched_pending_yocto
        {
            events::log_epoch_operation("epoch_settle_net_zero_state_changed", validator_id);
            return false;
        }
        validator.pending_to_stake = NearToken::from_near(0);
        validator.pending_to_unstake = NearToken::from_near(0);
        if matched_pending_yocto > 0 {
            validator.pending_to_claim = NearToken::from_yoctonear(
                validator
                    .pending_to_claim
                    .as_yoctonear()
                    .saturating_add(matched_pending_yocto),
            );
        }
        validator.last_settlement_epoch = epoch_height();
        self.internal_set_validator(validator_id.clone(), validator);
        true
    }

    // --- [Pipeline 3b] ---

    /// **[Pipeline 3b]** `deposit_and_stake` callback (stays **`Busy`**).
    #[private]
    pub fn on_deposit_and_stake(&mut self, validator_id: ValidatorId) -> bool {
        let ok = is_promise_success();
        if !ok {
            return false;
        }

        let mut validator = self.require_validator(&validator_id);
        let pending_stake_yocto = validator.pending_to_stake.as_yoctonear();
        let pending_unstake_yocto = validator.pending_to_unstake.as_yoctonear();
        require!(
            pending_stake_yocto > pending_unstake_yocto,
            "deposit_and_stake callback requires pending_to_stake > pending_to_unstake"
        );
        let net_stake_yocto = pending_stake_yocto - pending_unstake_yocto;
        if pending_unstake_yocto > 0 {
            validator.pending_to_claim = NearToken::from_yoctonear(
                validator
                    .pending_to_claim
                    .as_yoctonear()
                    .saturating_add(pending_unstake_yocto),
            );
        }
        validator.pending_to_stake = NearToken::from_near(0);
        validator.pending_to_unstake = NearToken::from_near(0);
        validator.total_staked_balance = validator
            .total_staked_balance
            .checked_add(NearToken::from_yoctonear(net_stake_yocto))
            .expect("total_staked_balance overflow after stake");
        validator.last_settlement_epoch = epoch_height();
        self.internal_set_validator(validator_id, validator);
        true
    }

    // --- [Pipeline 3c] ---

    /// **[Pipeline 3c]** `unstake` callback (stays **`Busy`**).
    #[private]
    pub fn on_unstake(&mut self, validator_id: ValidatorId) -> bool {
        let ok = is_promise_success();
        if !ok {
            return false;
        }

        let mut validator = self.require_validator(&validator_id);
        let current_epoch = epoch_height();
        let pending_stake_yocto = validator.pending_to_stake.as_yoctonear();
        let pending_unstake_yocto = validator.pending_to_unstake.as_yoctonear();
        require!(
            pending_unstake_yocto > pending_stake_yocto,
            "unstake callback requires pending_to_unstake > pending_to_stake"
        );
        let net_unstake_yocto = pending_unstake_yocto - pending_stake_yocto;
        if pending_stake_yocto > 0 {
            validator.pending_to_claim = NearToken::from_yoctonear(
                validator
                    .pending_to_claim
                    .as_yoctonear()
                    .saturating_add(pending_stake_yocto),
            );
        }
        validator.pending_to_unstake = NearToken::from_near(0);
        validator.pending_to_stake = NearToken::from_near(0);
        validator.pending_to_withdraw = NearToken::from_yoctonear(
            validator
                .pending_to_withdraw
                .as_yoctonear()
                .saturating_add(net_unstake_yocto),
        );
        validator.last_unstake_epoch = current_epoch;
        validator.last_settlement_epoch = current_epoch;
        self.internal_set_validator(validator_id, validator);
        true
    }

    // --- [Pipeline 4] ---

    /// **[Pipeline 4]** Fan-out to **5a** / **5b** / **5c**, then chain **6**.
    #[private]
    pub fn on_epoch_settlement_dispatch_continue(&mut self, cont: UserAction) -> Promise {
        enum ReleaseKind {
            Terminal,
            WithLockId,
            WithSubscriptionUpdateOutcome,
            WithFarmPosition,
        }

        let pipeline_validator_id = cont.validator_id().clone();
        let cont_for_release = cont.clone();
        let (tail, release_kind) = match cont {
            // Catalog purchase: mint shares / usage after validator state is fresh for this epoch.
            UserAction::CommitLock {
                validator_id,
                buyer,
                locked,
                duration_ns,
                order,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_LOCK_FINALLY_MINT)
                    .resolve_lock(buyer, locked, duration_ns, order, validator_id),
                ReleaseKind::WithLockId,
            ),
            UserAction::CommitRecurringSubscriptionLock {
                validator_id,
                buyer,
                locked,
                price_id,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_LOCK_FINALLY_MINT)
                    .resolve_recurring_subscription_lock_after_settle(
                        buyer,
                        locked,
                        price_id,
                        validator_id,
                    ),
                ReleaseKind::WithLockId,
            ),
            UserAction::SubscriptionUpdate {
                validator_id,
                buyer,
                deposit,
                target_price_id,
                target_amount,
                subscription_id,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_SUBSCRIPTION_UPDATE_AFTER_SETTLE)
                    .on_subscription_update_after_settle(
                        buyer,
                        deposit,
                        target_price_id,
                        target_amount,
                        subscription_id,
                        validator_id,
                    ),
                ReleaseKind::WithSubscriptionUpdateOutcome,
            ),
            UserAction::CommitFarmStake {
                validator_id,
                account_id,
                deposit,
                product_id,
                price_id,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_FARM_STAKE_AFTER_SETTLE)
                    .resolve_farm_stake(account_id, deposit, product_id, price_id, validator_id),
                ReleaseKind::WithFarmPosition,
            ),
            UserAction::FarmUnstakeQueue {
                validator_id,
                account_id,
                product_id,
                amount,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_FARM_UNSTAKE_AFTER_SETTLE)
                    .resolve_farm_unstake(account_id, product_id, validator_id, amount),
                ReleaseKind::Terminal,
            ),
            // Share exit: burn shares and queue `pending_to_unstake` (pool `unstake` on a later settlement).
            UserAction::UnlockQueueUnstake {
                lock_id,
                account_id,
                validator_id,
                shares_remove,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_UNLOCK_TAIL_AFTER_PRE_USER)
                    .resolve_unlock(lock_id, account_id, validator_id, shares_remove),
                ReleaseKind::Terminal,
            ),
            // User claim: move unlocked NEAR from `pending_to_claim` (see `withdraw.rs`).
            UserAction::WithdrawUserTransfer {
                account_id,
                validator_id,
            } => (
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_WITHDRAW_TAIL_AFTER_PRE_USER)
                    .on_withdraw_user_transfer_after_settle(account_id, validator_id),
                ReleaseKind::Terminal,
            ),
            UserAction::SettleOnly { .. } => {
                return ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_PIPELINE_TERMINAL_RELEASE)
                    .on_epoch_pipeline_terminal_release(pipeline_validator_id);
            }
        };

        match release_kind {
            ReleaseKind::Terminal => tail.then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_PIPELINE_TERMINAL_RELEASE)
                    .on_epoch_pipeline_terminal_release(pipeline_validator_id),
            ),
            ReleaseKind::WithLockId => tail.then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_LOCK_ID)
                    .on_epoch_pipeline_release_with_lock_id(
                        pipeline_validator_id,
                        cont_for_release,
                    ),
            ),
            ReleaseKind::WithSubscriptionUpdateOutcome => tail.then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(
                        callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_SUBSCRIPTION_UPDATE_OUTCOME,
                    )
                    .on_epoch_pipeline_release_with_subscription_update_outcome(
                        pipeline_validator_id,
                        cont_for_release,
                    ),
            ),
            ReleaseKind::WithFarmPosition => tail.then(
                ext_self_epoch::ext(env::current_account_id())
                    .with_static_gas(callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_FARM_POSITION)
                    .on_epoch_pipeline_release_with_farm_position(
                        pipeline_validator_id,
                        cont_for_release,
                    ),
            ),
        }
    }

    // Pipeline 5 tails are dispatched in [Pipeline 4] and implemented in:
    // `lock.rs` (5a), `unlock.rs` (5b), `withdraw.rs` (5c), `subscriptions.rs` (5d).

    // --- [Pipeline 6] ---

    /// Refund NEAR from a payable pipeline entry (`lock`, `update_subscription`) after pre-user
    /// settlement aborts (e.g. `get_account` failure). Clears **`Busy`** and returns the refund transfer.
    pub(crate) fn refund_payable_pipeline(
        &mut self,
        validator_id: &ValidatorId,
        buyer: AccountId,
        amount: NearToken,
    ) -> Promise {
        self.release_validator_pool_pipeline(validator_id);
        events::log_epoch_operation("epoch_payable_pipeline_refund", validator_id);
        Promise::new(buyer).transfer(amount)
    }

    /// Used by **6** (and error paths in `unlock.rs`).
    pub(crate) fn release_validator_pool_pipeline(&mut self, validator_id: &ValidatorId) {
        let mut validator = self.require_validator(validator_id);
        validator.tx_status = TransactionStatus::Idle;
        self.internal_set_validator(validator_id.clone(), validator);
    }

    /// **[Pipeline 6]** After **4** tail completes: clear pipeline **`Busy`**.
    #[private]
    pub fn on_epoch_pipeline_terminal_release(&mut self, validator_id: ValidatorId) {
        self.release_validator_pool_pipeline(&validator_id);
    }

    /// **[Pipeline 6]** Release pipeline and return lock id from mint tail; refund payable entry on tail failure.
    #[private]
    pub fn on_epoch_pipeline_release_with_lock_id(
        &mut self,
        #[callback_result] lock_id_result: Result<LockId, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<LockId> {
        match lock_id_result {
            Ok(lock_id) => {
                self.release_validator_pool_pipeline(&validator_id);
                PromiseOrValue::Value(lock_id)
            }
            Err(_) => {
                events::log_epoch_operation("epoch_lock_pipeline_tail_failed", &validator_id);
                let (buyer, amount) = cont
                    .payable_refund()
                    .expect("WithLockId continuation must be payable");
                self.refund_payable_pipeline(&validator_id, buyer, amount)
                    .into()
            }
        }
    }

    /// **[Pipeline 6]** Release pipeline and return farm position from stake tail; refund payable entry on tail failure.
    #[private]
    pub fn on_epoch_pipeline_release_with_farm_position(
        &mut self,
        #[callback_result] position_result: Result<FarmPosition, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<FarmPosition> {
        match position_result {
            Ok(position) => {
                self.release_validator_pool_pipeline(&validator_id);
                PromiseOrValue::Value(position)
            }
            Err(_) => {
                events::log_epoch_operation("epoch_farm_stake_pipeline_tail_failed", &validator_id);
                let (buyer, amount) = cont
                    .payable_refund()
                    .expect("farm stake continuation must be payable");
                self.refund_payable_pipeline(&validator_id, buyer, amount)
                    .into()
            }
        }
    }

    /// **[Pipeline 6]** Release pipeline and return subscription update outcome; refund payable entry on tail failure.
    #[private]
    pub fn on_epoch_pipeline_release_with_subscription_update_outcome(
        &mut self,
        #[callback_result] outcome_result: Result<SubscriptionPlanChangeOutcome, PromiseError>,
        validator_id: ValidatorId,
        cont: UserAction,
    ) -> PromiseOrValue<SubscriptionPlanChangeOutcome> {
        match outcome_result {
            Ok(outcome) => {
                self.release_validator_pool_pipeline(&validator_id);
                PromiseOrValue::Value(outcome)
            }
            Err(_) => {
                events::log_epoch_operation(
                    "epoch_subscription_update_pipeline_tail_failed",
                    &validator_id,
                );
                let (buyer, amount) = cont
                    .payable_refund()
                    .expect("subscription update continuation must be payable");
                self.refund_payable_pipeline(&validator_id, buyer, amount)
                    .into()
            }
        }
    }
}
