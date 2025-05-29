use crate::proposal::{ProposalInfo, ProposalStatus, SnapshotAndState};
use crate::*;
use common::global_state::{GlobalState, VGlobalState};
use common::{events, TimestampNs};
use near_sdk::{assert_one_yocto, ext_contract, Gas, Promise};
use std::ops::Mul;

pub const GAS_FOR_ON_GET_SNAPSHOT: Gas = Gas::from_tgas(30);

#[near]
impl Contract {
    /// Approves the proposal to start the voting process.
    /// An optional voting start time in seconds can be provided to delay the start of the voting.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the reviewers.
    #[payable]
    pub fn approve_proposal(
        &mut self,
        proposal_id: ProposalId,
        voting_start_time_sec: Option<u32>,
    ) -> Promise {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_reviewer();
        let proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        events::emit::approve_proposal_action(
            "proposal_approve",
            &env::predecessor_account_id(),
            proposal_id,
            voting_start_time_sec,
        );

        ext_venear::ext(self.config.venear_account_id.clone())
            .with_unused_gas_weight(1)
            .get_snapshot()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_ON_GET_SNAPSHOT)
                    .on_get_snapshot(
                        env::predecessor_account_id(),
                        proposal_id,
                        voting_start_time_sec,
                    ),
            )
    }

    /// Rejects the proposal.
    /// Requires 1 yocto attached to the call.
    /// Can only be called by the reviewers.
    #[payable]
    pub fn reject_proposal(&mut self, proposal_id: ProposalId) {
        assert_one_yocto();
        self.assert_not_paused();
        self.assert_called_by_reviewer();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        proposal.rejected = true;
        proposal.reviewer_id = Some(env::predecessor_account_id());
        proposal.status = ProposalStatus::Rejected;

        events::emit::approve_proposal_action(
            "proposal_reject",
            &env::predecessor_account_id(),
            proposal_id,
            None,
        );

        self.internal_set_proposal(proposal);
    }

    /// A callback after the snapshot is received for approving the proposal.
    #[private]
    pub fn on_get_snapshot(
        &mut self,
        #[callback] snapshot_and_state: (MerkleTreeSnapshot, VGlobalState),
        reviewer_id: AccountId,
        proposal_id: ProposalId,
        voting_start_time_sec: Option<u32>,
    ) -> ProposalInfo {
        self.assert_not_paused();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        let timestamp: TimestampNs = env::block_timestamp().into();

        proposal.reviewer_id = Some(reviewer_id);
        proposal.voting_start_time_ns = Some(
            voting_start_time_sec
                .map(|v| u64::from(v).mul(10u64.pow(9)).into())
                .unwrap_or(timestamp),
        );
        require!(
            proposal.voting_start_time_ns.unwrap() >= timestamp,
            "Voting start time is in the past."
        );

        let mut global_state: GlobalState = snapshot_and_state.1.into();
        global_state.update(timestamp.into());
        proposal.snapshot_and_state = Some(SnapshotAndState {
            snapshot: snapshot_and_state.0,
            timestamp_ns: timestamp.into(),
            total_venear: global_state.total_venear_balance.total(),
            venear_growth_config: global_state.venear_growth_config,
        });
        proposal.status = ProposalStatus::Approved;
        self.approved_proposals.push(proposal_id);

        self.internal_set_proposal(proposal.clone());

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
}

#[allow(dead_code)]
#[ext_contract(ext_venear)]
trait ExtVenear {
    fn get_snapshot(&self);
}

#[allow(dead_code)]
#[ext_contract(ext_self)]
trait ExtSelf {
    fn on_get_snapshot(
        &mut self,
        reviewer_id: AccountId,
        proposal_id: ProposalId,
        voting_start_time_sec: Option<u32>,
    );
}
