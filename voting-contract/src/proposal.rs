use crate::legacy::{ProposalV1, ProposalV2};
use crate::metadata::ProposalMetadata;
use crate::*;
use common::{Bps, TimestampNs, events, near_add, near_sub};
use near_sdk::json_types::{Base64VecU8, U64};
use near_sdk::{Gas, Promise};

pub use common::voting::{MajorityType, ProposalStatus, VoteOption};

pub type ProposalId = u32;

const NS_PER_DAY: u64 = 86_400_000_000_000;

/// Bytes reserved at proposal-creation time to cover the future `SnapshotAndState` write.
pub(crate) const SNAPSHOT_STORAGE_RESERVE_BYTES: u64 = 101;

/// A single action that the voting contract can execute on behalf of a passed proposal.
#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub enum ProposalAction {
    FunctionCall {
        receiver_id: AccountId,
        method_name: String,
        args: Base64VecU8,
        deposit: NearToken,
        gas: Gas,
    },
    Transfer {
        receiver_id: AccountId,
        amount: NearToken,
    },
}

/// Which proposal flow a proposal follows.
#[derive(Clone, Copy, Debug, PartialEq)]
#[near(serializers=[borsh, json])]
pub enum ProposalFlow {
    Classic,
    FastTrack,
}

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct Proposal {
    pub id: ProposalId,
    pub creation_time_ns: U64,
    pub proposer_id: AccountId,
    pub reviewer_id: Option<AccountId>,
    pub rejecter_id: Option<AccountId>,
    pub approval_time_ns: Option<U64>,
    pub voting_start_time_ns: Option<U64>,
    pub voting_duration_ns: U64,
    pub expiration_ns: U64,
    pub snapshot_and_state: Option<SnapshotAndState>,
    pub votes: Vec<VoteStats>,
    pub total_votes: VoteStats,
    pub status: ProposalStatus,
    pub quorum_threshold_bps: Bps,
    pub quorum_floor: NearToken,
    pub approval_threshold_bps: Bps,
    pub actions: Option<Vec<ProposalAction>>,
    pub flow: ProposalFlow,
    // Classic only
    pub timelock_duration_ns: U64,
    // FastTrack only
    pub sandbox_start_time_ns: Option<U64>,
    /// Unclaimed bond NEAR; reset to 0 once the bond is claimed.
    pub bond_amount: NearToken,
    pub sandbox_duration_ns: U64,
    pub sandbox_threshold_bps: Bps,
}

/// Borsh-tagged proposal storage envelope. `V1` (oldest) and `V2` (classic) hold legacy shapes,
/// converted to `Proposal` on read; `Current` wraps the live `Proposal`.
#[derive(Clone)]
#[near(serializers=[borsh])]
pub enum VProposal {
    V1(ProposalV1),
    V2(ProposalV2),
    Current(Proposal),
}

impl From<Proposal> for VProposal {
    fn from(current: Proposal) -> Self {
        Self::Current(current)
    }
}

impl From<VProposal> for Proposal {
    fn from(v: VProposal) -> Self {
        match v {
            VProposal::V1(p) => p.into(),
            VProposal::V2(p) => p.into(),
            VProposal::Current(p) => p,
        }
    }
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

/// Returns the next voting start time. In sandbox test mode, starts after 120 seconds.
/// Otherwise, starts on the next Monday 00:00 UTC strictly after `after_ns`.
pub fn next_voting_start_ns(after_ns: u64) -> u64 {
    if cfg!(feature = "sandbox") {
        after_ns + 120 * 1_000_000_000
    } else {
        let days_since_epoch = after_ns / NS_PER_DAY;
        let day_of_week = days_since_epoch % 7;
        let days_until_monday = (10 - day_of_week) % 7 + 1;
        (days_since_epoch + days_until_monday) * NS_PER_DAY
    }
}

impl Proposal {
    pub fn has_actions(&self) -> bool {
        self.actions.as_ref().is_some_and(|a| !a.is_empty())
    }

    /// Transitions a proposal into its first active status.
    pub fn activate(&mut self, start_time: U64) {
        match self.flow {
            ProposalFlow::Classic => {
                self.voting_start_time_ns = Some(start_time);
                self.status = ProposalStatus::Voting;
            }
            ProposalFlow::FastTrack => {
                self.sandbox_start_time_ns = Some(start_time);
                self.status = ProposalStatus::Sandbox;
            }
        }
    }

    /// Latest timestamp at which an active proposal still occupies its slot. Used by the
    /// scheduler to backdate the start of a queued proposal that fills a freed slot.
    pub fn active_end_time_ns(&self) -> u64 {
        match self.flow {
            ProposalFlow::Classic => {
                let voting_end = self.voting_start_time_ns.unwrap().0 + self.voting_duration_ns.0;
                if self.compute_final_status() == ProposalStatus::Succeeded {
                    voting_end + self.timelock_duration_ns.0
                } else {
                    voting_end
                }
            }
            ProposalFlow::FastTrack => {
                if let Some(voting_start) = self.voting_start_time_ns {
                    voting_start.0 + self.voting_duration_ns.0
                } else {
                    self.sandbox_start_time_ns.unwrap().0 + self.sandbox_duration_ns.0
                }
            }
        }
    }

    pub fn sandbox_threshold_met(&self) -> bool {
        let Some(snapshot) = self.snapshot_and_state.as_ref() else {
            return false;
        };
        let for_power = self
            .votes
            .first()
            .map(|v| v.total_venear)
            .unwrap_or(NearToken::from_yoctonear(0));
        let threshold = self.sandbox_threshold_bps * snapshot.total_venear;
        for_power >= threshold
    }

    pub fn update(&mut self, timestamp: TimestampNs) {
        match self.flow {
            ProposalFlow::Classic => self.update_classic(timestamp),
            ProposalFlow::FastTrack => self.update_fast_track(timestamp),
        }
    }

    fn update_classic(&mut self, timestamp: TimestampNs) {
        match self.status {
            ProposalStatus::Rejected
            | ProposalStatus::Vetoed
            | ProposalStatus::Succeeded
            | ProposalStatus::Expired
            | ProposalStatus::Defeated
            | ProposalStatus::Executable
            | ProposalStatus::InProgress
            | ProposalStatus::Failed
            | ProposalStatus::ApprovalLegacy
            | ProposalStatus::FinishLegacy => {}
            ProposalStatus::Created => {
                if self.expiration_ns.0 > 0 && timestamp.0 >= self.expiration_ns.0 {
                    self.status = ProposalStatus::Expired;
                }
            }
            ProposalStatus::Voting | ProposalStatus::Timelock => {
                let voting_end = self.voting_start_time_ns.unwrap().0 + self.voting_duration_ns.0;
                let timelock_end = voting_end + self.timelock_duration_ns.0;
                if timestamp.0 >= voting_end {
                    let final_status = self.compute_final_status();
                    self.status = if final_status != ProposalStatus::Succeeded {
                        final_status
                    } else if timestamp.0 < timelock_end {
                        ProposalStatus::Timelock
                    } else if self.has_actions() {
                        ProposalStatus::Executable
                    } else {
                        ProposalStatus::Succeeded
                    };
                }
            }
            ProposalStatus::Slashed | ProposalStatus::Sandbox | ProposalStatus::Scheduled => {
                // Not reachable for classic proposals.
            }
            ProposalStatus::Queued => {
                // Queued waits for explicit promotion by the scheduler.
            }
        }
    }

    fn update_fast_track(&mut self, timestamp: TimestampNs) {
        match self.status {
            ProposalStatus::Rejected
            | ProposalStatus::Succeeded
            | ProposalStatus::Expired
            | ProposalStatus::Defeated
            | ProposalStatus::Executable
            | ProposalStatus::InProgress
            | ProposalStatus::Failed
            | ProposalStatus::Slashed
            | ProposalStatus::Vetoed
            | ProposalStatus::ApprovalLegacy
            | ProposalStatus::FinishLegacy => {}
            ProposalStatus::Created => {
                if self.expiration_ns.0 > 0 && timestamp.0 >= self.expiration_ns.0 {
                    self.status = ProposalStatus::Expired;
                }
            }
            ProposalStatus::Sandbox => {
                let sandbox_end =
                    self.sandbox_start_time_ns.unwrap().0 + self.sandbox_duration_ns.0;
                if timestamp.0 >= sandbox_end {
                    self.status = ProposalStatus::Defeated;
                }
            }
            ProposalStatus::Scheduled | ProposalStatus::Voting => {
                let voting_start = self.voting_start_time_ns.unwrap().0;
                let voting_end = voting_start + self.voting_duration_ns.0;
                if timestamp.0 >= voting_end {
                    let final_status = self.compute_final_status();
                    self.status = if final_status != ProposalStatus::Succeeded {
                        final_status
                    } else if self.has_actions() {
                        ProposalStatus::Executable
                    } else {
                        ProposalStatus::Succeeded
                    };
                } else if timestamp.0 >= voting_start {
                    self.status = ProposalStatus::Voting;
                }
            }
            ProposalStatus::Timelock => {
                // Not reachable for FastTrack proposals.
            }
            ProposalStatus::Queued => {
                // Queued waits for explicit promotion by the scheduler.
            }
        }
    }

    pub fn compute_final_status(&self) -> ProposalStatus {
        let Some(snapshot) = self.snapshot_and_state.as_ref() else {
            return ProposalStatus::Defeated;
        };
        // Quorum check
        let bps_quorum = self.quorum_threshold_bps * snapshot.total_venear;
        let quorum_required = std::cmp::max(bps_quorum, self.quorum_floor);
        let quorum_met = self.total_votes.total_venear >= quorum_required;

        // Approval threshold check: votes[0] = For, votes[1] = Against
        let for_power = self
            .votes
            .first()
            .map(|v| v.total_venear.as_yoctonear())
            .unwrap_or(0);
        let against_power = self
            .votes
            .get(1)
            .map(|v| v.total_venear.as_yoctonear())
            .unwrap_or(0);
        let denominator = for_power + against_power;
        // Cross-multiply to avoid division: for * 10000 >= threshold * (for + against)
        let approval_met = denominator > 0
            && for_power * 10_000 >= u128::from(self.approval_threshold_bps) * denominator;

        if quorum_met && approval_met {
            ProposalStatus::Succeeded
        } else {
            ProposalStatus::Defeated
        }
    }
}

#[near]
impl Contract {
    /// Creates a new proposal. Deposit covers storage, `base_proposal_fee`, and (FastTrack only)
    /// `bond_amount`.
    #[payable]
    pub fn create_proposal(
        &mut self,
        metadata: ProposalMetadata,
        actions: Option<Vec<ProposalAction>>,
        flow: ProposalFlow,
    ) -> ProposalId {
        self.assert_not_paused();
        let attached_deposit = env::attached_deposit();

        require!(
            !actions.as_ref().is_some_and(|a| a.is_empty()),
            "Actions list cannot be empty"
        );

        let proposer_id = env::predecessor_account_id();
        let proposal_id = self.proposals.len();

        events::emit::create_proposal_action("create_proposal", &proposer_id, proposal_id);

        let creation_time_ns: u64 = env::block_timestamp();
        let (flow_expiration_ns, voting_duration_ns, timelock_duration_ns, bond_amount) = match flow
        {
            ProposalFlow::Classic => (
                self.config.classic_proposal_expiration_ns,
                self.config.classic_voting_duration_ns,
                self.config.timelock_duration_ns,
                NearToken::from_yoctonear(0),
            ),
            ProposalFlow::FastTrack => (
                self.config.fast_track_proposal_expiration_ns,
                self.config.fast_track_voting_duration_ns,
                U64(0),
                self.config.bond_amount,
            ),
        };
        let expiration_ns = if flow_expiration_ns.0 > 0 {
            U64(creation_time_ns + flow_expiration_ns.0)
        } else {
            U64(0)
        };
        let proposal = Proposal {
            id: proposal_id,
            creation_time_ns: creation_time_ns.into(),
            proposer_id,
            reviewer_id: None,
            rejecter_id: None,
            voting_start_time_ns: None,
            voting_duration_ns,
            expiration_ns,
            snapshot_and_state: None,
            votes: vec![VoteStats::default(); 3],
            total_votes: VoteStats::default(),
            status: ProposalStatus::Created,
            quorum_threshold_bps: Bps::ZERO,
            quorum_floor: NearToken::from_yoctonear(0),
            approval_threshold_bps: Bps::ZERO,
            actions,
            flow,
            approval_time_ns: None,
            timelock_duration_ns,
            sandbox_start_time_ns: None,
            bond_amount,
            sandbox_duration_ns: U64(0),
            sandbox_threshold_bps: Bps::ZERO,
        };
        let storage_usage = env::storage_usage();
        self.proposals.push(proposal.into());
        self.proposals.flush();
        self.proposal_metadata.push(metadata.into());
        self.proposal_metadata.flush();
        let updated_storage_usage = env::storage_usage() + SNAPSHOT_STORAGE_RESERVE_BYTES;
        let storage_added = updated_storage_usage.saturating_sub(storage_usage);
        let storage_added_cost = env::storage_byte_cost()
            .checked_mul(u128::from(storage_added))
            .unwrap();
        let required_deposit = near_add(
            near_add(bond_amount, self.config.base_proposal_fee),
            storage_added_cost,
        );
        require!(
            attached_deposit >= required_deposit,
            format!(
                "Requires deposit of {}",
                required_deposit.exact_amount_display()
            )
        );
        if attached_deposit > required_deposit {
            let refund = near_sub(attached_deposit, required_deposit);
            Promise::new(env::predecessor_account_id())
                .transfer(refund)
                .detach();
        }
        proposal_id
    }

    pub fn get_proposal(&self, proposal_id: ProposalId) -> Option<ProposalInfo> {
        let proposal = self
            .get_proposals_virtual_updates()
            .remove(&proposal_id)
            .or_else(|| self.internal_get_proposal(proposal_id).map(|(p, _)| p))?;
        Some(ProposalInfo {
            proposal,
            metadata: self.proposal_metadata.get(proposal_id)?.clone().into(),
        })
    }

    /// Returns the number of proposals.
    pub fn get_num_proposals(&self) -> u32 {
        self.proposals.len()
    }

    pub fn get_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo> {
        let mut overrides = self.get_proposals_virtual_updates();
        let limit = limit.unwrap_or(u32::MAX);
        let to_index = std::cmp::min(from_index.saturating_add(limit), self.get_num_proposals());
        (from_index..to_index)
            .map(|i| {
                let proposal = overrides
                    .remove(&i)
                    .unwrap_or_else(|| self.internal_get_proposal(i).unwrap().0);
                ProposalInfo {
                    proposal,
                    metadata: self.proposal_metadata.get(i).unwrap().clone().into(),
                }
            })
            .collect()
    }
}

/// A proposal occupies an active slot while it is in Sandbox, Timelock, Scheduled, or Voting status.
pub fn is_active_status(status: ProposalStatus) -> bool {
    matches!(
        status,
        ProposalStatus::Sandbox
            | ProposalStatus::Scheduled
            | ProposalStatus::Voting
            | ProposalStatus::Timelock
    )
}

impl Contract {
    pub fn internal_set_proposal(&mut self, proposal: Proposal) {
        let proposal_id = proposal.id;
        let new_active = is_active_status(proposal.status);
        let was_active = self.active_proposals.contains(&proposal_id);
        if !was_active && new_active {
            self.active_proposals.insert(proposal_id);
        } else if was_active && !new_active {
            self.active_proposals.remove(&proposal_id);
        }

        self.proposals[proposal_id] = proposal.into();
    }

    pub fn internal_get_proposal(&self, proposal_id: ProposalId) -> Option<(Proposal, bool)> {
        self.proposals.get(proposal_id).cloned().map(|proposal| {
            let mut proposal: Proposal = proposal.into();
            let old_status = proposal.status;
            proposal.update(env::block_timestamp().into());
            let changed = proposal.status != old_status;
            (proposal, changed)
        })
    }

    pub fn internal_expect_proposal_updated(&self, proposal_id: ProposalId) -> Proposal {
        self.internal_get_proposal(proposal_id)
            .expect(format!("Proposal {} is not found", proposal_id).as_str())
            .0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit_tests::test_utils::date_ns;

    #[test]
    fn test_next_monday_from_each_weekday() {
        // 2026-04-06 is Monday UTC, next Monday UTC is 2026-04-13.
        let expected = date_ns(2026, 4, 13);

        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 6)), expected); // Monday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 7)), expected); // Tuesday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 8)), expected); // Wednesday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 9)), expected); // Thursday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 10)), expected); // Friday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 11)), expected); // Saturday
        assert_eq!(next_voting_start_ns(date_ns(2026, 4, 12)), expected); // Sunday
    }

    #[test]
    fn test_next_monday_from_time_within_day() {
        // All inputs lie within Tuesday 2026-04-07 UTC; result is next Monday UTC.
        let expected = date_ns(2026, 4, 13);
        let day = date_ns(2026, 4, 7);
        assert_eq!(next_voting_start_ns(day), expected);
        assert_eq!(next_voting_start_ns(day + 1), expected);
        assert_eq!(
            next_voting_start_ns(day + 12 * 3600 * 1_000_000_000),
            expected
        );
        assert_eq!(
            next_voting_start_ns(day + 24 * 3600 * 1_000_000_000 - 1),
            expected
        );
    }
}
