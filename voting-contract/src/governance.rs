use crate::*;
use common::Bps;
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

    /// Updates the Classic-flow voting duration in seconds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_classic_voting_duration(&mut self, voting_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.classic_voting_duration_ns =
            (u64::from(voting_duration_sec) * 10u64.pow(9)).into();
    }

    /// Updates the FastTrack-flow voting duration in seconds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_fast_track_voting_duration(&mut self, voting_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.fast_track_voting_duration_ns =
            (u64::from(voting_duration_sec) * 10u64.pow(9)).into();
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

    /// Updates the list of council member account IDs who can veto proposals during timelock.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_council_ids(&mut self, council_ids: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        self.config.council_ids = council_ids;
    }

    /// Updates the timelock duration in seconds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_timelock_duration(&mut self, timelock_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.timelock_duration_ns = (u64::from(timelock_duration_sec) * 10u64.pow(9)).into();
    }

    /// Updates the Classic proposal expiration duration in seconds.
    /// Set to 0 to disable proposal expiration.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_classic_proposal_expiration(&mut self, proposal_expiration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.classic_proposal_expiration_ns =
            (u64::from(proposal_expiration_sec) * 10u64.pow(9)).into();
    }

    /// Updates the FastTrack proposal expiration duration in seconds.
    /// Set to 0 to disable proposal expiration.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_fast_track_proposal_expiration(&mut self, proposal_expiration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.fast_track_proposal_expiration_ns =
            (u64::from(proposal_expiration_sec) * 10u64.pow(9)).into();
    }

    /// Updates the quorum threshold in basis points (e.g. 3500 = 35%).
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_quorum_threshold_bps(&mut self, quorum_threshold_bps: Bps) {
        assert_one_yocto();
        self.assert_owner();
        self.config.quorum_threshold_bps = quorum_threshold_bps;
    }

    /// Updates the quorum floor (absolute minimum veNEAR required for quorum).
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_quorum_floor(&mut self, quorum_floor: NearToken) {
        assert_one_yocto();
        self.assert_owner();
        self.config.quorum_floor = quorum_floor;
    }

    /// TODO: remove with the next state migration.
    /// The value no longer affects approvals — use the simple/strong majority setters instead.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_approval_threshold_bps(&mut self, approval_threshold_bps: Bps) {
        assert_one_yocto();
        self.assert_owner();
        self.config.approval_threshold_bps = approval_threshold_bps;
    }

    /// Updates the FastTrack bond amount required to create a proposal.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_bond_amount(&mut self, bond_amount: NearToken) {
        assert_one_yocto();
        self.assert_owner();
        self.config.bond_amount = bond_amount;
    }

    /// Updates the treasury account ID that receives forfeited FastTrack bonds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_treasury_account_id(&mut self, treasury_account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.config.treasury_account_id = treasury_account_id;
    }

    /// Updates the simple majority threshold in basis points (e.g. 5000 = 50%).
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_simple_majority_threshold_bps(&mut self, simple_majority_threshold_bps: Bps) {
        assert_one_yocto();
        self.assert_owner();
        self.config.simple_majority_threshold_bps = simple_majority_threshold_bps;
    }

    /// Updates the strong (super) majority threshold in basis points (e.g. 6667 ≈ 66.67%).
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_strong_majority_threshold_bps(&mut self, strong_majority_threshold_bps: Bps) {
        assert_one_yocto();
        self.assert_owner();
        self.config.strong_majority_threshold_bps = strong_majority_threshold_bps;
    }

    /// Updates the FastTrack sandbox duration in seconds.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_sandbox_duration(&mut self, sandbox_duration_sec: u32) {
        assert_one_yocto();
        self.assert_owner();
        self.config.sandbox_duration_ns = (u64::from(sandbox_duration_sec) * 10u64.pow(9)).into();
    }

    /// Updates the FastTrack sandbox threshold in basis points (e.g. 3000 = 30%).
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_sandbox_threshold_bps(&mut self, sandbox_threshold_bps: Bps) {
        assert_one_yocto();
        self.assert_owner();
        self.config.sandbox_threshold_bps = sandbox_threshold_bps;
    }

    /// Updates the maximum number of simultaneously active (Sandbox/Scheduled/Voting) proposals.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn set_max_active_proposals(&mut self, max_active_proposals: u32) {
        assert_one_yocto();
        self.assert_owner();
        require!(
            max_active_proposals > 0,
            "max_active_proposals must be greater than 0"
        );
        self.config.max_active_proposals = max_active_proposals;
        self.internal_advance_queue();
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
