use crate::*;
use near_sdk::assert_one_yocto;

#[near]
impl Contract {
    /// Updates the account ID of the veNEAR contract.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_venear_account_id(&mut self, venear_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.venear_account_id = venear_account_id;
    }

    /// Updates the list of account IDs that can review proposals.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_reviewer_ids(&mut self, reviewer_ids: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        self.config.reviewer_ids = reviewer_ids;
    }

    /// Updates the maximum duration of the voting period in seconds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_voting_duration(&mut self, voting_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.voting_duration_ns = (voting_duration_sec as u64 * 10u64.pow(9)).into();
    }

    /// Updates the base fee required to create a proposal.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_base_proposal_fee(&mut self, base_proposal_fee: NearToken) {
        assert_one_yocto();
        self.assert_owner();
        self.config.base_proposal_fee = base_proposal_fee;
    }

    /// Updates the storage fee required to store a vote for an active proposal.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    /// Will panic if called, because it requires contract upgrade to change the storage fee.
    #[payable]
    pub fn set_vote_storage_fee(&mut self, _vote_storage_fee: NearToken) {
        // The reason for this restriction is that the contract needs to be upgraded to change the
        // storage fee. Otherwise, the storage for the previous votes will be refunded with the
        // new storage fee.
        env::panic_str("Vote storage fee cannot be changed, without contract upgrade");
    }

    /// Updates the maximum number of voting options per proposal.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_max_number_of_voting_options(&mut self, max_number_of_voting_options: u8) {
        assert_one_yocto();
        self.assert_owner();
        self.config.max_number_of_voting_options = max_number_of_voting_options;
    }

    /// Sets the account ID that can upgrade the current contract and modify the config.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_owner_account_id(&mut self, owner_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.owner_account_id = owner_account_id;
    }

    /// Sets the list of account IDs that can pause the contract.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_guardians(&mut self, guardians: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        self.config.guardians = guardians;
    }
}

impl Contract {
    pub fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.config.owner_account_id,
            "Only the owner can call this method"
        );
    }

    /// Asserts that the caller is one of the guardians or the owner.
    pub fn assert_guardian(&self) {
        let predecessor = env::predecessor_account_id();
        require!(
            self.config.guardians.contains(&predecessor)
                || predecessor == self.config.owner_account_id,
            "Only the guardian can call this method"
        );
    }
}
