use crate::*;
use near_sdk::assert_one_yocto;
use near_sdk::json_types::Base58CryptoHash;

impl Contract {
    pub fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.config.owner_account_id,
            "Only the owner can call this method"
        );
    }
}
