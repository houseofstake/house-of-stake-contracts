use crate::*;
use common::{near_add, near_sub, TimestampNs};
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

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct Proposal {
    /// The unique identifier of the proposal, generated automatically.
    pub id: ProposalId,
    /// The timestamp in nanoseconds when the proposal was created, generated automatically.
    pub creation_time_ns: U64,
    /// The account ID of the proposer.
    pub proposer_id: AccountId,
    /// The metadata of the proposal, provided by the proposer.
    pub metadata: ProposalMetadata,
    /// The timestamp when the voting starts, provided by the approver.
    pub voting_start_time_ns: Option<U64>,
    /// The voting duration in nanoseconds, generated from the config.
    pub voting_duration_ns: U64,
    /// The flag indicating if the proposal was rejected by the approver.
    pub rejected: bool,
    /// The snapshot of the contract state and global state. Fetched when the proposal is approved.
    pub snapshot_and_state: Option<SnapshotAndState>,
    /// Aggregated votes per voting option.
    pub votes: Vec<VoteStats>,
    /// The total aggregated voting information across all voting options.
    pub total_votes: VoteStats,
    /// The status of the proposal. It's optional and can be computed from the proposal itself.
    pub status: ProposalStatus,
}

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
}

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

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct ProposalMetadata {
    /// The title of the proposal.
    pub title: Option<String>,

    /// The description of the proposal.
    pub description: Option<String>,

    /// The link to the proposal.
    pub link: Option<String>,

    /// The voting options for the proposal.
    pub voting_options: Vec<String>,
}

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
            ProposalStatus::Created | ProposalStatus::Rejected | ProposalStatus::Finished => {
                return;
            }
            ProposalStatus::Approved | ProposalStatus::Voting => {
                if timestamp.0 >= self.voting_start_time_ns.unwrap().0 + self.voting_duration_ns.0 {
                    self.status = ProposalStatus::Finished;
                } else if timestamp >= self.voting_start_time_ns.unwrap() {
                    self.status = ProposalStatus::Voting;
                }
            }
        }
    }
}

#[near]
impl Contract {
    #[payable]
    pub fn create_proposal(&mut self, metadata: ProposalMetadata) -> ProposalId {
        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit == self.config.proposal_fee,
            format!(
                "Requires deposit of {}",
                self.config.proposal_fee.exact_amount_display()
            )
        );

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

        let proposer_id = env::predecessor_account_id();
        let proposal_id = self.proposals.len();
        let proposal = Proposal {
            id: proposal_id,
            creation_time_ns: env::block_timestamp().into(),
            proposer_id,
            metadata,
            voting_start_time_ns: None,
            voting_duration_ns: self.config.voting_duration_ns,
            rejected: false,
            snapshot_and_state: None,
            votes: vec![VoteStats::default(); num_voting_options],
            total_votes: VoteStats::default(),
            status: ProposalStatus::Created,
        };
        self.proposals.push(proposal.into());
        proposal_id
    }

    #[payable]
    pub fn vote(
        &mut self,
        proposal_id: ProposalId,
        vote: u32,
        merkle_proof: MerkleProof,
        v_account: VAccount,
    ) {
        let attached_deposit = env::attached_deposit();
        require!(!attached_deposit.is_zero(), "Requires attached deposit");

        let mut proposal: Proposal = self.internal_expect_proposal_updated(proposal_id);

        match proposal.status {
            ProposalStatus::Voting => {}
            ProposalStatus::Created | ProposalStatus::Approved => {
                env::panic_str("Voting is not started yet");
            }
            ProposalStatus::Rejected => {
                env::panic_str("Proposal is rejected");
            }
            ProposalStatus::Finished => {
                env::panic_str("Voting is finished");
            }
        }

        // Validate merkle proof
        {
            let SnapshotAndState { snapshot, .. } = proposal.snapshot_and_state.as_ref().unwrap();
            merkle_proof.verify(snapshot.root.into(), snapshot.length, &v_account);
        }

        let timestamp_ns = proposal.snapshot_and_state.as_ref().unwrap().timestamp_ns;
        let account: Account = v_account.into();
        let account_id = &account.account_id;
        let account_balance = account
            .venear_balance(
                timestamp_ns,
                &proposal
                    .snapshot_and_state
                    .as_ref()
                    .unwrap()
                    .venear_growth_config,
            )
            .total();
        require!(!account_balance.is_zero(), "Account has no veNEAR balance");

        let previous_vote = self.votes.get(&(account_id.clone(), proposal_id)).cloned();
        require!(
            previous_vote != Some(vote),
            "Already voted for the same option"
        );
        let mut storage_added = self.config.vote_storage_fee;
        if let Some(previous_vote) = previous_vote {
            proposal.votes[previous_vote as usize].remove_vote(account_balance);
            proposal.total_votes.remove_vote(account_balance);
            // When changing the vote. Don't need to charge the fee again.
            storage_added = NearToken::from_yoctonear(0);
        }
        proposal.votes[vote as usize].add_vote(account_balance);
        proposal.total_votes.add_vote(account_balance);

        require!(
            attached_deposit >= storage_added,
            format!(
                "Requires deposit of {}",
                storage_added.exact_amount_display()
            )
        );

        // Note, don't refund 1 yoctoNEAR if changing the vote.
        if attached_deposit > near_add(storage_added, NearToken::from_yoctonear(1)) {
            let refund = near_sub(attached_deposit, storage_added);
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }

        self.votes.insert((account_id.clone(), proposal_id), vote);
        self.internal_set_proposal(proposal);
    }

    pub fn get_proposal(&self, proposal_id: ProposalId) -> Option<Proposal> {
        self.internal_get_proposal(proposal_id)
    }
}

impl Contract {
    pub fn internal_set_proposal(&mut self, proposal: Proposal) {
        let proposal_id = proposal.id;
        self.proposals[proposal_id] = proposal.into();
    }

    pub fn internal_get_proposal(&self, proposal_id: ProposalId) -> Option<Proposal> {
        self.proposals
            .get(proposal_id)
            .cloned()
            .map(|proposal| proposal.into())
    }

    pub fn internal_expect_proposal_updated(&self, proposal_id: ProposalId) -> Proposal {
        let mut proposal = self
            .internal_get_proposal(proposal_id)
            .expect(format!("Proposal {} is not found", proposal_id).as_str());
        proposal.update(env::block_timestamp().into());
        proposal
    }
}
