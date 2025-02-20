use crate::*;
use near_sdk::near;

#[near]
impl LockupContract {
    /// Returns the account ID of the owner.
    pub fn get_owner_account_id(&self) -> AccountId {
        self.owner_account_id.clone()
    }

    /// Returns the account ID of the selected staking pool.
    pub fn get_staking_pool_account_id(&self) -> Option<AccountId> {
        self.staking_information
            .as_ref()
            .map(|info| info.staking_pool_account_id.clone())
    }

    /// Returns the amount of tokens that were deposited to the staking pool.
    /// NOTE: The actual balance can be larger than this known deposit balance due to staking
    /// rewards acquired on the staking pool.
    /// To refresh the amount the owner can call `refresh_staking_pool_balance`.
    pub fn get_known_deposited_balance(&self) -> WrappedBalance {
        self.staking_information
            .as_ref()
            .map(|info| info.deposit_amount.0)
            .unwrap_or(0)
            .into()
    }

    /// Returns the amount of tokens that are locked in the account due to lockup or vesting.
    pub fn get_locked_amount(&self) -> WrappedBalance {
        let lockup_amount = self.lockup_information.lockup_amount;

        let lockup_timestamp = std::cmp::max(
            self.lockup_information
                .lockup_timestamp
                .unwrap_or(0)
                .saturating_add(self.lockup_information.lockup_duration),
            self.lockup_information.lockup_timestamp.unwrap_or(0),
        );

        let block_timestamp = env::block_timestamp();

        if lockup_timestamp <= block_timestamp {
            let unreleased_amount =
                if let &Some(release_duration) = &self.lockup_information.release_duration {
                    let end_timestamp = lockup_timestamp.saturating_add(release_duration);
                    if block_timestamp >= end_timestamp {
                        // Everything is released
                        0
                    } else {
                        let time_left = U256::from(end_timestamp - block_timestamp);
                        let unreleased_amount =
                            U256::from(lockup_amount) * time_left / U256::from(release_duration);
                        // The unreleased amount can't be larger than lockup_amount because the
                        // time_left is smaller than total_time.
                        unreleased_amount.as_u128()
                    }
                } else {
                    0
                };

            return unreleased_amount.into();
        }

        // The entire balance is still locked before the lockup timestamp.
        lockup_amount.into()
    }

    /// Returns the balance of the account owner. It includes vested and extra tokens that
    /// may have been deposited to this account, but excludes locked tokens.
    /// NOTE: Some of this tokens may be deposited to the staking pool.
    /// This method also doesn't account for tokens locked for the contract storage.
    pub fn get_owners_balance(&self) -> WrappedBalance {
        (env::account_balance().as_yoctonear() + self.get_known_deposited_balance().0)
            .saturating_sub(self.get_locked_amount().0)
            .into()
    }

    /// Returns total balance of the account including tokens deposited to the staking pool.
    pub fn get_balance(&self) -> WrappedBalance {
        (env::account_balance().as_yoctonear() + self.get_known_deposited_balance().0).into()
    }

    /// Returns the amount of tokens the owner can transfer from the account.
    /// Transfers have to be enabled.
    pub fn get_liquid_owners_balance(&self) -> WrappedBalance {
        std::cmp::min(self.get_owners_balance().0, self.get_account_balance().0).into()
    }

    /// Returns the version of the Lockup contract.
    pub fn get_version(&self) -> Version {
        self.version.clone()
    }
}
