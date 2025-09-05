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

    /// Updates the maximum number of voting options per proposal.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_max_number_of_voting_options(&mut self, max_number_of_voting_options: u8) {
        assert_one_yocto();
        self.assert_owner();
        self.config.max_number_of_voting_options = max_number_of_voting_options;
    }

    /// Proposes the new owner account ID.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn propose_new_owner_account_id(&mut self, new_owner_account_id: Option<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        self.config.proposed_new_owner_account_id = new_owner_account_id;
    }

    /// Accepts the new owner account ID.
    /// Can only be called by the new owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn accept_ownership(&mut self) {
        assert_one_yocto();
        let predecessor = env::predecessor_account_id();
        require!(
            self.config.proposed_new_owner_account_id.as_ref() == Some(&predecessor),
            "Only the proposed new owner can call this method"
        );
        self.config.owner_account_id = predecessor;
        self.config.proposed_new_owner_account_id = None;
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

    /// Updates the default quorum percentage required for proposals to pass.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_default_quorum_percentage(&mut self, default_quorum_percentage: u8) {
        assert_one_yocto();
        self.assert_owner();
        require!(
            default_quorum_percentage <= 100,
            "Quorum percentage must be between 0 and 100"
        );
        self.config.default_quorum_percentage = default_quorum_percentage;
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
