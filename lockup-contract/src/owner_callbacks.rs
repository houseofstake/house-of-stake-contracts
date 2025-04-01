use crate::*;
use near_sdk::{is_promise_success, near, PromiseOrValue};

#[near]
impl LockupContract {
    /// Called after a given `staking_pool_account_id` was checked in the whitelist.
    #[private]
    pub fn on_whitelist_is_whitelisted(
        &mut self,
        #[callback] is_whitelisted: bool,
        staking_pool_account_id: AccountId,
    ) -> bool {
        assert!(
            is_whitelisted,
            "The given staking pool account ID is not whitelisted"
        );
        self.assert_staking_pool_is_not_selected();
        self.staking_information = Some(StakingInformation {
            staking_pool_account_id,
            status: TransactionStatus::Idle,
            deposit_amount: NearToken::from_yoctonear(0),
        });
        true
    }

    /// Called after a deposit amount was transferred out of this account to the staking pool.
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_deposit(&mut self, amount: NearToken) -> bool {
        let deposit_succeeded = is_promise_success();
        self.on_staking_pool_deposit_inner(amount, deposit_succeeded)
    }

    /// Called after a deposit amount was transferred out of this account to the staking pool and it
    /// was staked on the staking pool.
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_deposit_and_stake(&mut self, amount: NearToken) -> bool {
        let deposit_and_stake_succeeded = is_promise_success();
        self.set_staking_pool_status(TransactionStatus::Idle);

        if deposit_and_stake_succeeded {
            self.staking_information.as_mut().unwrap().deposit_amount = NearToken::from_yoctonear(
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .deposit_amount
                    .as_yoctonear()
                    + amount.as_yoctonear(),
            );

            env::log_str(&format!(
                "The deposit and stake of {} to @{} succeeded",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "The deposit and stake of {} to @{} has failed",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        deposit_and_stake_succeeded
    }

    /// Called after the given amount was requested to transfer out from the staking pool to this
    /// account.
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_withdraw(&mut self, amount: NearToken) -> bool {
        let withdraw_succeeded = is_promise_success();
        self.on_staking_pool_withdraw_inner(amount, withdraw_succeeded)
    }

    /// Called after the extra amount stake was staked in the staking pool contract.
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_stake(&mut self, amount: NearToken) -> bool {
        let stake_succeeded = is_promise_success();
        self.on_staking_pool_stake_inner(amount, stake_succeeded)
    }

    /// Called after the given amount was unstaked at the staking pool contract.
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_unstake(&mut self, amount: NearToken) -> bool {
        let unstake_succeeded = is_promise_success();
        self.on_staking_pool_unstake_inner(amount, unstake_succeeded)
    }

    /// Called after all tokens were unstaked at the staking pool contract
    /// This method needs to update staking pool status.
    #[private]
    pub fn on_staking_pool_unstake_all(&mut self) -> bool {
        let unstake_all_succeeded = is_promise_success();
        self.set_staking_pool_status(TransactionStatus::Idle);

        if unstake_all_succeeded {
            env::log_str(&format!(
                "Unstaking all at @{} succeeded",
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "Unstaking all at @{} has failed",
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        unstake_all_succeeded
    }

    /// Called after the request to get the current total balance from the staking pool.
    #[private]
    pub fn on_get_account_total_balance(&mut self, #[callback] total_balance: NearToken) {
        self.set_staking_pool_status(TransactionStatus::Idle);

        env::log_str(&format!(
            "The current total balance on the staking pool is {}",
            total_balance
        ));

        self.staking_information.as_mut().unwrap().deposit_amount = total_balance;
    }

    /// Called after the request to get the current unstaked balance to withdraw everything by th
    /// owner.
    #[private]
    pub fn on_get_account_unstaked_balance_to_withdraw_by_owner(
        &mut self,
        #[callback] unstaked_balance: NearToken,
    ) -> PromiseOrValue<bool> {
        if unstaked_balance.as_yoctonear() > 0 {
            // Need to withdraw
            env::log_str(&format!(
                "Withdrawing {} from the staking pool @{}",
                unstaked_balance,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));

            ext_staking_pool::ext(
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
                    .clone(),
            )
            .with_static_gas(gas::staking_pool::WITHDRAW)
            .withdraw(unstaked_balance)
            .then(
                ext_self_owner::ext(env::current_account_id())
                    .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_WITHDRAW)
                    .on_staking_pool_withdraw(unstaked_balance),
            )
            .into()
        } else {
            env::log_str("No unstaked balance on the staking pool to withdraw");
            self.set_staking_pool_status(TransactionStatus::Idle);
            PromiseOrValue::Value(true)
        }
    }
}

impl LockupContract {
    pub fn on_staking_pool_deposit_inner(
        &mut self,
        amount: NearToken,
        deposit_succeeded: bool,
    ) -> bool {
        self.set_staking_pool_status(TransactionStatus::Idle);

        if deposit_succeeded {
            self.staking_information.as_mut().unwrap().deposit_amount = NearToken::from_yoctonear(
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .deposit_amount
                    .as_yoctonear()
                    + amount.as_yoctonear(),
            );
            env::log_str(&format!(
                "The deposit of {} to @{} succeeded",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "The deposit of {} to @{} has failed",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        deposit_succeeded
    }

    pub fn on_staking_pool_stake_inner(
        &mut self,
        amount: NearToken,
        stake_succeeded: bool,
    ) -> bool {
        self.set_staking_pool_status(TransactionStatus::Idle);

        if stake_succeeded {
            env::log_str(&format!(
                "Staking of {} at @{} succeeded",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "Staking {} at @{} has failed",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        stake_succeeded
    }

    pub fn on_staking_pool_unstake_inner(
        &mut self,
        amount: NearToken,
        unstake_succeeded: bool,
    ) -> bool {
        self.set_staking_pool_status(TransactionStatus::Idle);

        if unstake_succeeded {
            env::log_str(&format!(
                "Unstaking of {} at @{} succeeded",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "Unstaking {} at @{} has failed",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        unstake_succeeded
    }

    pub fn on_staking_pool_withdraw_inner(
        &mut self,
        amount: NearToken,
        withdraw_succeeded: bool,
    ) -> bool {
        self.set_staking_pool_status(TransactionStatus::Idle);

        if withdraw_succeeded {
            {
                let staking_information = self.staking_information.as_mut().unwrap();
                // Due to staking rewards the deposit amount can become negative.
                staking_information.deposit_amount = NearToken::from_yoctonear(
                    staking_information
                        .deposit_amount
                        .as_yoctonear()
                        .saturating_sub(amount.as_yoctonear()),
                );
            }
            env::log_str(&format!(
                "The withdrawal of {} from @{} succeeded",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        } else {
            env::log_str(&format!(
                "The withdrawal of {} from @{} failed",
                amount,
                self.staking_information
                    .as_ref()
                    .unwrap()
                    .staking_pool_account_id
            ));
        }
        withdraw_succeeded
    }
}
