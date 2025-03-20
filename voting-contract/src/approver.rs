use crate::proposal::{Proposal, ProposalStatus, SnapshotAndState};
use crate::*;
use common::global_state::{GlobalState, VGlobalState};
use common::{events, TimestampNs};
use near_sdk::{assert_one_yocto, ext_contract, Gas, Promise};
use std::ops::Mul;

pub const GAS_FOR_ON_GET_SNAPSHOT: Gas = Gas::from_tgas(30);

#[near]
impl Contract {
    #[payable]
    pub fn approve_proposal(
        &mut self,
        proposal_id: ProposalId,
        voting_start_time_sec: Option<u32>,
    ) -> Promise {
        assert_one_yocto();
        self.assert_called_by_approver();
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
                    .on_get_snapshot(proposal_id, voting_start_time_sec),
            )
    }

    #[payable]
    pub fn reject_proposal(&mut self, proposal_id: ProposalId) {
        assert_one_yocto();
        self.assert_called_by_approver();
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        proposal.rejected = true;
        proposal.status = ProposalStatus::Rejected;

        events::emit::approve_proposal_action(
            "proposal_reject",
            &env::predecessor_account_id(),
            proposal_id,
            None,
        );

        self.internal_set_proposal(proposal);
    }

    #[private]
    pub fn on_get_snapshot(
        &mut self,
        #[callback] snapshot_and_state: (MerkleTreeSnapshot, VGlobalState),
        proposal_id: ProposalId,
        voting_start_time_sec: Option<u32>,
    ) -> Proposal {
        let mut proposal = self.internal_expect_proposal_updated(proposal_id);

        if proposal.status != ProposalStatus::Created {
            env::panic_str("Proposal is not in the Created status");
        }

        let timestamp: TimestampNs = env::block_timestamp().into();

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

        self.internal_set_proposal(proposal.clone());

        proposal
    }
}

impl Contract {
    pub fn assert_called_by_approver(&self) {
        require!(
            env::predecessor_account_id() == self.config.approver_id,
            "Only the approver can call this method"
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
    fn on_get_snapshot(&mut self, proposal_id: ProposalId, voting_start_time_sec: Option<u32>);
}
