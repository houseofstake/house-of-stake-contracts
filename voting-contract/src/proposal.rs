use crate::metadata::ProposalMetadata;
use crate::*;
use common::{events, near_add, near_sub, TimestampNs};
use near_sdk::json_types::U64;
use near_sdk::Promise;

pub type ProposalId = u32;

#[derive(Clone)]
#[near(serializers=[borsh])]
pub enum VProposal {
    Current(Proposal),
}

impl From<Proposal> for VProposal {
    fn from(current: Proposal) -> Self {
        Self::Current(current)
    }
}

impl From<VProposal> for Proposal {
    fn from(value: VProposal) -> Self {
        match value {
            VProposal::Current(current) => current,
        }
    }
}

/// The proposal structure that contains all the information about a proposal.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct Proposal {
    /// The unique identifier of the proposal, generated automatically.
    pub id: ProposalId,
    /// The timestamp in nanoseconds when the proposal was created, generated automatically.
    pub creation_time_ns: U64,
    /// The account ID of the proposer.
    pub proposer_id: AccountId,
    /// The account ID of the reviewer, who approved or rejected the proposal.
    pub reviewer_id: Option<AccountId>,
    /// The timestamp when the voting starts, provided by the reviewer.
    pub voting_start_time_ns: Option<U64>,
    /// The voting duration in nanoseconds, generated from the config.
    pub voting_duration_ns: U64,
    /// The flag indicating if the proposal was rejected by the reviewer.
    pub rejected: bool,
    /// The snapshot of the contract state and global state. Fetched when the proposal is approved.
    pub snapshot_and_state: Option<SnapshotAndState>,
    /// Aggregated votes per voting option.
    pub votes: Vec<VoteStats>,
    /// The total aggregated voting information across all voting options.
    pub total_votes: VoteStats,
    /// The status of the proposal. It's optional and can be computed from the proposal itself.
    pub status: ProposalStatus,
    /// The quorum percentage (0-100) required for this proposal.
    pub quorum_percentage: u8,
}

/// The proposal information structure that contains the proposal and its metadata.
#[derive(Clone)]
#[near(serializers=[json])]
pub struct ProposalInfo {
    #[serde(flatten)]
    pub proposal: Proposal,
    #[serde(flatten)]
    pub metadata: ProposalMetadata,
}

/// The status of the proposal
#[derive(Clone, Copy, PartialEq)]
#[near(serializers=[borsh, json])]
pub enum ProposalStatus {
    /// The proposal was created and is waiting for the approver to approve or reject it.
    Created,
    /// The proposal was rejected by the approver.
    Rejected,
    /// The proposal was approved by the approver and is waiting for the voting to start.
    Approved,
    /// The proposal is in the voting phase.
    Voting,
    /// The proposal voting is finished and the results are available.
    Finished,
    /// The proposal voting is finished but quorum was not met.
    QuorumNotMet,
}

/// The snapshot of the Merkle tree and the global state at the moment when the proposal was
/// approved.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct SnapshotAndState {
    /// The snapshot of the Merkle tree at the moment when the proposal was approved.
    pub snapshot: MerkleTreeSnapshot,
    /// The timestamp in nanoseconds when the global state was last updated.
    pub timestamp_ns: TimestampNs,
    /// The total amount of veNEAR tokens at the moment when the proposal was approved.
    pub total_venear: NearToken,
    /// The growth configuration of the veNEAR tokens from the global state.
    pub venear_growth_config: VenearGrowthConfig,
}

/// The vote statistics structure that contains the total amount of veNEAR tokens and the total
/// number of votes.
#[derive(Clone, Default)]
#[near(serializers=[borsh, json])]
pub struct VoteStats {
    /// The total venear balance at the updated timestamp.
    pub total_venear: NearToken,

    /// The total number of votes.
    pub total_votes: u32,
}

impl VoteStats {
    pub fn add_vote(&mut self, venear: NearToken) {
        self.total_votes += 1;
        self.total_venear = near_add(self.total_venear, venear);
    }

    pub fn remove_vote(&mut self, venear: NearToken) {
        self.total_votes -= 1;
        self.total_venear = near_sub(self.total_venear, venear);
    }
}

impl Proposal {
    pub fn update(&mut self, timestamp: TimestampNs) {
        match self.status {
            ProposalStatus::Created | ProposalStatus::Rejected | ProposalStatus::Finished | ProposalStatus::QuorumNotMet => {
                return;
            }
            ProposalStatus::Approved | ProposalStatus::Voting => {
                if timestamp.0 >= self.voting_start_time_ns.unwrap().0 + self.voting_duration_ns.0 {
                    // Check if quorum is met
                    if let Some(snapshot_and_state) = &self.snapshot_and_state {
                        let required_venear = snapshot_and_state.total_venear.as_yoctonear() * self.quorum_percentage as u128 / 100;
                        if self.total_votes.total_venear.as_yoctonear() >= required_venear {
                            self.status = ProposalStatus::Finished;
                        } else {
                            self.status = ProposalStatus::QuorumNotMet;
                        }
                    } else {
                        // Should not happen, but fallback to finished
                        self.status = ProposalStatus::Finished;
                    }
                } else if timestamp >= self.voting_start_time_ns.unwrap() {
                    self.status = ProposalStatus::Voting;
                }
            }
        }
    }
}

#[near]
impl Contract {
    /// Creates a new proposal with the given metadata.
    /// The proposal is created by the predecessor account and requires a deposit to cover the
    /// storage and the base proposal fee.
    #[payable]
    pub fn create_proposal(&mut self, metadata: ProposalMetadata) -> ProposalId {
        self.assert_not_paused();
        let attached_deposit = env::attached_deposit();
        let num_voting_options = metadata.voting_options.len();

        require!(
            num_voting_options >= 2,
            "Requires at least 2 voting options"
        );

        require!(
            num_voting_options <= self.config.max_number_of_voting_options as usize,
            format!(
                "Too many voting options, max is {}",
                self.config.max_number_of_voting_options
            )
        );

        // Validate quorum percentage if provided
        if let Some(quorum_pct) = metadata.quorum_percentage {
            require!(
                quorum_pct <= 100,
                "Quorum percentage must be between 0 and 100"
            );
        }

        let proposer_id = env::predecessor_account_id();
        let proposal_id = self.proposals.len();

        events::emit::create_proposal_action(
            "create_proposal",
            &proposer_id,
            proposal_id,
            &metadata.title,
            &metadata.description,
            &metadata.link,
            &metadata.voting_options,
        );

        let proposal = Proposal {
            id: proposal_id,
            creation_time_ns: env::block_timestamp().into(),
            proposer_id,
            reviewer_id: None,
            voting_start_time_ns: None,
            voting_duration_ns: self.config.voting_duration_ns,
            rejected: false,
            snapshot_and_state: None,
            votes: vec![VoteStats::default(); num_voting_options],
            total_votes: VoteStats::default(),
            status: ProposalStatus::Created,
            quorum_percentage: metadata.quorum_percentage.unwrap_or(self.config.default_quorum_percentage),
        };
        let storage_usage = env::storage_usage();
        self.proposals.push(proposal.into());
        self.proposals.flush();
        self.proposal_metadata.push(metadata.into());
        self.proposal_metadata.flush();
        let updated_storage_usage = env::storage_usage();
        let storage_added = updated_storage_usage.saturating_sub(storage_usage);
        let storage_added_cost = env::storage_byte_cost()
            .checked_mul(storage_added as _)
            .unwrap();
        let required_deposit = near_add(self.config.base_proposal_fee, storage_added_cost);
        require!(
            attached_deposit >= required_deposit,
            format!(
                "Requires deposit of {}",
                required_deposit.exact_amount_display()
            )
        );
        if attached_deposit > required_deposit {
            let refund = near_sub(attached_deposit, required_deposit);
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }
        proposal_id
    }

    /// Returns the proposal information by the given proposal ID.
    pub fn get_proposal(&self, proposal_id: ProposalId) -> Option<ProposalInfo> {
        self.internal_get_proposal(proposal_id)
            .map(|proposal| ProposalInfo {
                proposal,
                metadata: self
                    .proposal_metadata
                    .get(proposal_id)
                    .unwrap()
                    .clone()
                    .into(),
            })
    }

    /// Returns the number of proposals.
    pub fn get_num_proposals(&self) -> u32 {
        self.proposals.len()
    }

    /// Returns a list of proposals from the given index based on the proposal ID order.
    pub fn get_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo> {
        let from_index = from_index;
        let limit = limit.unwrap_or(u32::MAX);
        let to_index = std::cmp::min(from_index.saturating_add(limit), self.get_num_proposals());
        (from_index..to_index)
            .into_iter()
            .filter_map(|i| self.get_proposal(i))
            .collect()
    }

    /// Returns the number of approved proposals.
    pub fn get_num_approved_proposals(&self) -> u32 {
        self.approved_proposals.len()
    }

    /// Returns a list of approved proposals from the given index based on the approved proposals
    /// order.
    pub fn get_approved_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo> {
        let from_index = from_index;
        let limit = limit.unwrap_or(u32::MAX);
        let to_index = std::cmp::min(
            from_index.saturating_add(limit),
            self.get_num_approved_proposals(),
        );
        (from_index..to_index)
            .into_iter()
            .filter_map(|i| self.get_proposal(self.approved_proposals[i]))
            .collect()
    }
}

impl Contract {
    pub fn internal_set_proposal(&mut self, proposal: Proposal) {
        let proposal_id = proposal.id;
        self.proposals[proposal_id] = proposal.into();
    }

    pub fn internal_get_proposal(&self, proposal_id: ProposalId) -> Option<Proposal> {
        self.proposals.get(proposal_id).cloned().map(|proposal| {
            let mut proposal: Proposal = proposal.into();
            proposal.update(env::block_timestamp().into());
            proposal
        })
    }

    pub fn internal_expect_proposal_updated(&self, proposal_id: ProposalId) -> Proposal {
        self.internal_get_proposal(proposal_id)
            .expect(format!("Proposal {} is not found", proposal_id).as_str())
    }
}
