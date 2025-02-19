use crate::*;
use near_sdk::NearToken;

/********************/
/* Internal methods */
/********************/

impl LockupContract {
    /// The balance of the account excluding the storage staking balance.
    /// NOTE: The storage staking balance can't be transferred out without deleting this contract.
    pub fn get_account_balance(&self) -> WrappedBalance {
        env::account_balance()
            .saturating_sub(NearToken::from_yoctonear(MIN_BALANCE_FOR_STORAGE))
            .as_yoctonear()
            .into()
    }

    pub fn set_staking_pool_status(&mut self, status: TransactionStatus) {
        self.staking_information
            .as_mut()
            .expect("Staking pool should be selected")
            .status = status;
    }

    pub fn assert_no_staking_or_idle(&self) {
        if let Some(staking_information) = &self.staking_information {
            match staking_information.status {
                TransactionStatus::Idle => (),
                TransactionStatus::Busy => {
                    env::panic_str("Contract is currently busy with another operation")
                }
            };
        }
    }

    pub fn assert_staking_pool_is_idle(&self) {
        assert!(
            self.staking_information.is_some(),
            "Staking pool is not selected"
        );
        match self.staking_information.as_ref().unwrap().status {
            TransactionStatus::Idle => (),
            TransactionStatus::Busy => {
                env::panic_str("Contract is currently busy with another operation")
            }
        };
    }

    pub fn assert_staking_pool_is_not_selected(&self) {
        assert!(
            self.staking_information.is_none(),
            "Staking pool is already selected"
        );
    }

    pub fn assert_owner(&self) {
        assert_eq!(
            &env::predecessor_account_id(),
            &self.owner_account_id,
            "Can only be called by the owner"
        )
    }
}
