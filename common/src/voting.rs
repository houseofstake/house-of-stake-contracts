use crate::*;

pub type ProposalId = u32;

/// The fixed voting options for proposals.
#[derive(Clone, Copy, PartialEq)]
#[near(serializers=[borsh, json])]
pub enum VoteOption {
    For,
    Against,
    Abstain,
}

impl From<VoteOption> for u8 {
    fn from(v: VoteOption) -> u8 {
        match v {
            VoteOption::For => 0,
            VoteOption::Against => 1,
            VoteOption::Abstain => 2,
        }
    }
}

/// The majority type required for a proposal to pass.
#[derive(Clone, Copy, PartialEq)]
#[near(serializers=[borsh, json])]
pub enum MajorityType {
    /// Simple majority (e.g. >50%).
    Simple,
    /// Strong majority (e.g. >66.67%).
    Strong,
}

/// Snapshot of the proposal scheduler: currently-active proposals and the FIFO pending queue.
#[derive(Debug, PartialEq, Eq)]
#[near(serializers=[json])]
pub struct QueueState {
    pub active_proposals: Vec<ProposalId>,
    pub pending_queue: Vec<ProposalId>,
}

/// The status of the proposal
#[derive(Clone, Copy, Debug, PartialEq)]
#[near(serializers=[borsh, json])]
pub enum ProposalStatus {
    /// The proposal was created and is waiting for the approver to approve it.
    Created,
    /// The proposal was rejected by a reviewer before approval.
    Rejected,
    /// Legacy: the proposal was approved by the approver and is waiting for the voting to start.
    ApprovalLegacy,
    /// The proposal is in the voting phase.
    Voting,
    /// Legacy: the proposal voting is finished and the results are available.
    FinishLegacy,
    /// The proposal was vetoed by a council member during the timelock period.
    Vetoed,
    /// The voting has ended and the proposal is in the timelock period awaiting potential council veto.
    Timelock,
    /// The proposal expired before being approved by a reviewer.
    Expired,
    /// The proposal voting has finished, quorum was met and approval threshold was met.
    Succeeded,
    /// The proposal voting has finished, but quorum was not met or approval threshold was not met.
    Defeated,
    /// The proposal passed and has actions ready for on-chain execution.
    Executable,
    /// The proposal actions are being executed (dispatched, awaiting callback).
    InProgress,
    /// The proposal's on-chain execution failed.
    Failed,
    /// The proposal was slashed by a reviewer; bond is forfeited to the treasury.
    Slashed,
    /// Graduates to Scheduled when the sandbox threshold is met.
    Sandbox,
    /// The proposal met the sandbox threshold and is scheduled to start voting.
    Scheduled,
    /// Approved by a reviewer but waiting for an active slot to open.
    Queued,
}
