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
    pub fn get_known_deposited_balance(&self) -> NearToken {
        self.staking_information
            .as_ref()
            .map(|info| info.deposit_amount)
            .unwrap_or(NearToken::from_yoctonear(0))
    }

    /// Returns the balance of the account owner. It includes vested and extra tokens that
    /// may have been deposited to this account, but excludes locked tokens.
    /// NOTE: Some of this tokens may be deposited to the staking pool.
    /// This method also doesn't account for tokens locked for the contract storage.
    pub fn get_owners_balance(&self) -> NearToken {
        NearToken::from_yoctonear(
            env::account_balance().as_yoctonear()
                + self.get_known_deposited_balance().as_yoctonear(),
        )
    }

    /// Returns total balance of the account including tokens deposited to the staking pool.
    pub fn get_balance(&self) -> NearToken {
        NearToken::from_yoctonear(
            env::account_balance().as_yoctonear()
                + self.get_known_deposited_balance().as_yoctonear(),
        )
    }

    /// Returns the amount of tokens the owner can transfer from the account.
    /// Transfers have to be enabled.
    pub fn get_liquid_owners_balance(&self) -> NearToken {
        std::cmp::min(self.get_owners_balance(), self.get_account_balance()).into()
    }

    /// Returns the version of the Lockup contract.
    pub fn get_version(&self) -> Version {
        self.version
    }
}
