use crate::proposal::{
    MajorityType, ProposalFlow, ProposalInfo, ProposalStatus, SNAPSHOT_STORAGE_RESERVE_BYTES,
    SnapshotAndState,
};
use crate::*;
use common::global_state::{GlobalState, VGlobalState};
use common::{TimestampNs, events};
use near_sdk::{Gas, Promise, PromiseOrValue, assert_one_yocto};

pub const GAS_FOR_ON_GET_SNAPSHOT: Gas = Gas::from_tgas(30);

#[near]
impl Contract {
    /// Approves a proposal.
    /// Requires 1 yocto attached.
    /// Reviewers only.
    #[payable]
    pub fn approve_proposal(
        &mut self,
        proposal_id: ProposalId,
        majority_type: MajorityType,
    ) -> PromiseOrValue<Option<ProposalInfo>> {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_reviewer();
        self.internal_advance_queue();

        let mut proposal = self.internal_expect_proposal_updated(proposal_id);
        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        events::emit::approve_proposal_action(&env::predecessor_account_id(), proposal_id);

        proposal.reviewer_id = Some(env::predecessor_account_id());
        proposal.approval_time_ns = Some(env::block_timestamp().into());
        proposal.quorum_threshold_bps = self.config.quorum_threshold_bps;
        proposal.quorum_floor = self.config.quorum_floor;

        if proposal.bond_amount > NearToken::ZERO {
            Promise::new(self.config.treasury_account_id.clone())
                .transfer(proposal.bond_amount)
                .detach();
            proposal.bond_amount = NearToken::ZERO;
        }
        proposal.approval_threshold_bps = match majority_type {
            MajorityType::Simple => self.config.simple_majority_threshold_bps,
            MajorityType::Strong => self.config.strong_majority_threshold_bps,
        };
        match proposal.flow {
            ProposalFlow::Classic => {}
            ProposalFlow::FastTrack => {
                proposal.sandbox_duration_ns = self.config.sandbox_duration_ns;
                proposal.sandbox_threshold_bps = self.config.sandbox_threshold_bps;
            }
        }

        if self.active_proposals.len() < self.config.max_active_proposals {
            proposal.activate(env::block_timestamp().into());
            self.internal_set_proposal(proposal);
            PromiseOrValue::Promise(
                ext_venear::ext(self.config.venear_account_id.clone())
                    .with_unused_gas_weight(1)
                    .get_snapshot()
                    .then(
                        ext_self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_ON_GET_SNAPSHOT)
                            .on_get_snapshot(proposal_id),
                    ),
            )
        } else {
            proposal.status = ProposalStatus::Queued;
            self.internal_set_proposal(proposal);
            self.pending_queue.push(proposal_id);
            PromiseOrValue::Value(self.get_proposal(proposal_id))
        }
    }

    /// Rejects the proposal before it has been approved.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the reviewers while the proposal is in the Created status.
    #[payable]
    pub fn reject_proposal(&mut self, proposal_id: ProposalId) {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_reviewer();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal can only be rejected while in the Created status");
        }

        proposal.reviewer_id = Some(env::predecessor_account_id());
        proposal.status = ProposalStatus::Rejected;

        events::emit::reject_proposal_action(&env::predecessor_account_id(), proposal_id);

        self.internal_set_proposal(proposal);
    }

    /// Vetoes a proposal.
    /// * Classic flow: only valid during the Timelock period.
    /// * FastTrack flow: valid while the proposal is Scheduled or Voting.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the council members.
    #[payable]
    pub fn veto_proposal(&mut self, proposal_id: ProposalId) {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_council();
        self.internal_advance_queue();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        match (proposal.flow, proposal.status) {
            (ProposalFlow::Classic, ProposalStatus::Timelock) => {}
            (ProposalFlow::FastTrack, ProposalStatus::Voting | ProposalStatus::Scheduled) => {}
            (_, _) => env::panic_str("Proposal cannot be vetoed in its current state"),
        }

        proposal.rejecter_id = Some(env::predecessor_account_id());
        proposal.status = ProposalStatus::Vetoed;

        events::emit::veto_proposal_action(&env::predecessor_account_id(), proposal_id);

        self.internal_set_proposal(proposal);
        self.internal_advance_queue();
    }

    /// Waives the veto right during the timelock period, ending the timelock immediately so the
    /// proposal can advance to the next step.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the council members.
    #[payable]
    pub fn noveto_proposal(&mut self, proposal_id: ProposalId) {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_council();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Timelock {
            env::panic_str("Proposal can only be noveto'd during the timelock period");
        }

        proposal.status = if proposal.has_actions() {
            ProposalStatus::Executable
        } else {
            ProposalStatus::Succeeded
        };

        events::emit::noveto_proposal_action(&env::predecessor_account_id(), proposal_id);

        self.internal_set_proposal(proposal);
        self.internal_advance_queue();
    }

    /// Slashes a proposal that is still in Created status.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the reviewers.
    #[payable]
    pub fn slash_proposal(&mut self, proposal_id: ProposalId) -> PromiseOrValue<()> {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_reviewer();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        require!(
            proposal.flow == ProposalFlow::FastTrack,
            "Only FastTrack proposals can be slashed"
        );
        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal can only be slashed while in Created status");
        }

        proposal.status = ProposalStatus::Slashed;

        events::emit::slash_proposal_action(&env::predecessor_account_id(), proposal_id);

        let result = if proposal.bond_amount > NearToken::ZERO {
            let promise = Promise::new(self.config.treasury_account_id.clone())
                .transfer(proposal.bond_amount);
            proposal.bond_amount = NearToken::ZERO;
            PromiseOrValue::Promise(promise)
        } else {
            PromiseOrValue::Value(())
        };

        self.internal_set_proposal(proposal);
        result
    }

    /// Callback that stores the fetched veNEAR snapshot.
    #[private]
    pub fn on_get_snapshot(
        &mut self,
        #[callback] snapshot_and_state: (MerkleTreeSnapshot, VGlobalState),
        proposal_id: ProposalId,
    ) -> ProposalInfo {
        self.assert_not_paused();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Sandbox && proposal.status != ProposalStatus::Voting {
            env::panic_str("Proposal must be in Sandbox or Voting status");
        }
        if proposal.snapshot_and_state.is_some() {
            env::panic_str("Snapshot is already set for this proposal");
        }

        let timestamp: TimestampNs = env::block_timestamp().into();
        let mut global_state: GlobalState = snapshot_and_state.1.into();
        global_state.update(timestamp);
        proposal.snapshot_and_state = Some(SnapshotAndState {
            snapshot: snapshot_and_state.0,
            timestamp_ns: timestamp,
            total_venear: global_state.total_venear_balance.total(),
            venear_growth_config: global_state.venear_growth_config,
        });

        // Guard the reserve pre-paid at creation against future layout growth.
        let storage_before = env::storage_usage();
        self.internal_set_proposal(proposal);
        self.proposals.flush();
        let storage_added = env::storage_usage().saturating_sub(storage_before);
        require!(
            storage_added <= SNAPSHOT_STORAGE_RESERVE_BYTES,
            "Snapshot write exceeded reserved storage"
        );
        self.get_proposal(proposal_id).unwrap()
    }
}

impl Contract {
    pub fn assert_called_by_reviewer(&self) {
        require!(
            self.config
                .reviewer_ids
                .contains(&env::predecessor_account_id()),
            "Only the reviewers can call this method"
        );
    }

    pub fn assert_called_by_council(&self) {
        require!(
            self.config
                .council_ids
                .contains(&env::predecessor_account_id()),
            "Only the council can call this method"
        );
    }
}
