use crate::*;
use common::Bps;
use near_sdk::json_types::U64;

/// The configuration of the voting contract.
#[derive(Debug, Clone)]
#[near(serializers=[borsh, json])]
pub struct Config {
    /// The account ID of the veNEAR contract.
    pub venear_account_id: AccountId,

    /// The account IDs that can approve proposals.
    pub reviewer_ids: Vec<AccountId>,

    /// The account IDs of the council members who can veto proposals.
    pub council_ids: Vec<AccountId>,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,

    /// The maximum duration of the voting period in nanoseconds for Classic proposals.
    pub classic_voting_duration_ns: U64,

    /// The maximum duration of the voting period in nanoseconds for FastTrack proposals.
    pub fast_track_voting_duration_ns: U64,

    /// The duration of the timelock period in nanoseconds after voting ends.
    pub timelock_duration_ns: U64,

    /// The base fee in addition to the storage fee required to create a proposal.
    pub base_proposal_fee: NearToken,

    /// The bond amount required to create a FastTrack proposal.
    pub bond_amount: NearToken,

    /// The account ID that receives forfeited bonds.
    pub treasury_account_id: AccountId,

    /// Storage fee required to store a vote.
    pub vote_storage_fee: NearToken,

    /// The list of account IDs that can pause the contract.
    pub guardians: Vec<AccountId>,

    /// The maximum time in nanoseconds a Classic proposal can stay in Created status before expiring.
    /// 0 means no expiration.
    pub classic_proposal_expiration_ns: U64,

    /// The maximum time in nanoseconds a FastTrack proposal can stay in Created status before
    /// expiring. 0 means no expiration.
    pub fast_track_proposal_expiration_ns: U64,

    /// Proposed new owner account ID. The account has to accept ownership.
    pub proposed_new_owner_account_id: Option<AccountId>,

    /// Quorum threshold in basis points (e.g. 3500 = 35% of total veNEAR supply).
    pub quorum_threshold_bps: Bps,

    /// Absolute minimum veNEAR required for quorum, regardless of BPS calculation.
    pub quorum_floor: NearToken,

    /// TODO: remove with the next state migration.
    /// Unused since approval requires an explicitmajority type.
    pub approval_threshold_bps: Bps,

    /// Simple majority threshold in basis points (e.g. 5000 = 50%). Selectable at approval time.
    pub simple_majority_threshold_bps: Bps,

    /// Strong (super) majority threshold in basis points (e.g. 6667 ≈ 66.67%). Selectable at approval time.
    pub strong_majority_threshold_bps: Bps,

    /// The duration of the sandbox pre-voting period in nanoseconds for FastTrack proposals.
    pub sandbox_duration_ns: U64,

    /// The "For" votes threshold to graduate a FastTrack proposal from Sandbox to Scheduled.
    pub sandbox_threshold_bps: Bps,

    /// Maximum number of proposals allowed in Sandbox/Scheduled/Voting/Timelock simultaneously.
    /// Extra approved proposals park in the pending queue until a slot frees.
    pub max_active_proposals: u32,
}

#[near]
impl Contract {
    /// Returns the current contract configuration.
    pub fn get_config(&self) -> &Config {
        &self.config
    }
}
