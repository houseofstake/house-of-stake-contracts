use crate::*;
use near_sdk::assert_one_yocto;
use near_sdk::json_types::{Base58CryptoHash, U64};

#[near]
impl Contract {
    #[payable]
    pub fn set_lockup_contract(
        &mut self,
        contract_hash: Base58CryptoHash,
        min_lockup_deposit: NearToken,
    ) {
        assert_one_yocto();
        self.assert_owner();
        self.internal_set_lockup(contract_hash.into());
        self.config.min_lockup_deposit = min_lockup_deposit;
    }

    #[payable]
    pub fn set_local_deposit(&mut self, local_deposit: NearToken) {
        assert_one_yocto();
        self.assert_owner();
        self.config.local_deposit = local_deposit;
    }

    #[payable]
    pub fn set_staking_pool_whitelist_account_id(
        &mut self,
        staking_pool_whitelist_account_id: AccountId,
    ) {
        assert_one_yocto();
        self.assert_owner();
        self.config.staking_pool_whitelist_account_id = staking_pool_whitelist_account_id;
    }

    #[payable]
    pub fn set_owner_account_id(&mut self, owner_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.owner_account_id = owner_account_id;
    }

    #[payable]
    pub fn set_unlock_duration_ns(&mut self, unlock_duration_ns: U64) {
        assert_one_yocto();
        self.assert_owner();
        self.config.unlock_duration_ns = unlock_duration_ns;
    }

    #[payable]
    pub fn set_lockup_code_deployers(&mut self, lockup_code_deployers: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        self.config.lockup_code_deployers = lockup_code_deployers;
    }
}

impl Contract {
    pub fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.config.owner_account_id,
            "Only the owner can call this method"
        );
    }
}
