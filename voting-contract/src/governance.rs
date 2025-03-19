use crate::*;
use near_sdk::assert_one_yocto;

#[near]
impl Contract {
    #[payable]
    pub fn set_venear_account_id(&mut self, venear_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.venear_account_id = venear_account_id;
    }

    #[payable]
    pub fn set_approver_id(&mut self, approver_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.approver_id = approver_id;
    }

    #[payable]
    pub fn set_voting_duration(&mut self, voting_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.voting_duration_ns = (voting_duration_sec as u64 * 10u64.pow(9)).into();
    }

    #[payable]
    pub fn set_base_proposal_fee(&mut self, base_proposal_fee: NearToken) {
        assert_one_yocto();
        self.assert_owner();
        self.config.base_proposal_fee = base_proposal_fee;
    }

    #[payable]
    pub fn set_vote_storage_fee(&mut self, _vote_storage_fee: NearToken) {
        // The reason for this restriction is that the contract needs to be upgraded to change the
        // storage fee. Otherwise, the storage for the previous votes will be refunded with the
        // new storage fee.
        env::panic_str("Vote storage fee cannot be changed, without contract upgrade");
    }

    #[payable]
    pub fn set_max_number_of_voting_options(&mut self, max_number_of_voting_options: u16) {
        assert_one_yocto();
        self.assert_owner();
        self.config.max_number_of_voting_options = max_number_of_voting_options;
    }

    #[payable]
    pub fn set_owner_account_id(&mut self, owner_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.owner_account_id = owner_account_id;
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
