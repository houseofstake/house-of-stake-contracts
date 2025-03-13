use crate::*;
use near_sdk::assert_one_yocto;
use near_sdk::json_types::Base58CryptoHash;

#[near]
impl Contract {
    #[payable]
    pub fn set_lockup_contract(
        &mut self,
        contract_hash: Base58CryptoHash,
        min_extra_lockup_deposit: NearToken,
    ) {
        assert_one_yocto();
        self.assert_owner();
        self.internal_set_lockup(contract_hash.into());
        self.config.min_extra_lockup_deposit = min_extra_lockup_deposit;
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
