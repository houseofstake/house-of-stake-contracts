//! Reviewer/council proposal transitions across allowed/disallowed statuses and access guards.
use crate::proposal::ProposalFlow;
use crate::proposal::{MajorityType, ProposalStatus};
use crate::unit_tests::test_utils::*;
use crate::*;
use common::Bps;
use near_sdk::PromiseOrValue;
use near_sdk::json_types::U64;

// approve_proposal

#[test]
#[should_panic(expected = "Only the reviewers can call this method")]
fn approve_requires_reviewer() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(acc("rando.test.near"), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn approve_requires_one_yocto() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 0, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
}

#[test]
#[should_panic(expected = "Proposal is not in the Created status")]
fn approve_twice_panics_when_already_active() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, None);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
}

#[test]
fn approve_classic_applies_configured_thresholds() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, None);
    let cfg = default_config();
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(p.quorum_threshold_bps, cfg.quorum_threshold_bps);
    assert_eq!(p.quorum_floor, cfg.quorum_floor);
    assert_eq!(p.approval_threshold_bps, cfg.simple_majority_threshold_bps);
    assert_eq!(p.reviewer_id.as_ref(), Some(&reviewer()));
    assert_eq!(p.approval_time_ns, Some(U64(TEST_NOW_NS)));
}

#[test]
fn approve_classic_simple_majority_uses_simple_threshold() {
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_simple_majority_threshold_bps(Bps::new(5_100));
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(p.approval_threshold_bps, Bps::new(5_100));
}

#[test]
fn approve_classic_strong_majority_uses_strong_threshold() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Strong);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(
        p.approval_threshold_bps,
        default_config().strong_majority_threshold_bps
    );
}

#[test]
fn approve_fasttrack_simple_majority_uses_simple_threshold() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(
        p.approval_threshold_bps,
        default_config().simple_majority_threshold_bps
    );
}

#[test]
fn approve_fasttrack_strong_majority_uses_strong_threshold() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Strong);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(
        p.approval_threshold_bps,
        default_config().strong_majority_threshold_bps
    );
}

#[test]
fn approve_fasttrack_zeroes_bond_amount() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(p.bond_amount, NearToken::ZERO);
}

#[test]
fn approve_queues_when_active_slots_full() {
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let _first = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, _first, None);
    let second = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, second, None);

    assert_eq!(
        contract.get_proposal(second).unwrap().proposal.status,
        ProposalStatus::Queued
    );
    let queue = contract.get_queue_state();
    assert_eq!(queue.pending_queue, vec![second]);
}

// reject_proposal

#[test]
fn reject_moves_proposal_to_rejected() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    contract.reject_proposal(id);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(p.status, ProposalStatus::Rejected);
    assert_eq!(p.reviewer_id.as_ref(), Some(&reviewer()));
}

#[test]
#[should_panic(expected = "Only the reviewers can call this method")]
fn reject_requires_reviewer() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(acc("rando.test.near"), 1, TEST_NOW_NS);
    contract.reject_proposal(id);
}

#[test]
#[should_panic(expected = "Proposal can only be rejected while in the Created status")]
fn reject_panics_for_already_approved_proposal() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, None);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    contract.reject_proposal(id);
}

// veto_proposal

#[test]
fn veto_classic_during_timelock_succeeds() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

    // Read at voting_end so update flips Voting -> Timelock.
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(voter(), 0, voting_end);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Timelock
    );

    set_ctx(council(), 1, voting_end);
    contract.veto_proposal(id);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Vetoed
    );
}

#[test]
#[should_panic(expected = "Proposal cannot be vetoed in its current state")]
fn veto_fasttrack_during_sandbox_panics() {
    // FastTrack veto is allowed only during Scheduled or Voting, not Sandbox.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, id, Some(&fixture));
    set_ctx(council(), 1, TEST_NOW_NS);
    contract.veto_proposal(id);
}

#[test]
fn veto_fasttrack_during_scheduled_succeeds() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Scheduled
    );

    set_ctx(council(), 1, TEST_NOW_NS);
    contract.veto_proposal(id);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Vetoed
    );
}

#[test]
#[should_panic(expected = "Only the council can call this method")]
fn veto_requires_council() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    contract.veto_proposal(id);
}

#[test]
fn veto_fasttrack_during_voting_succeeds() {
    // Advance to next-Monday voting_start so the proposal is in Voting, then veto.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

    let voting_start = crate::proposal::next_voting_start_ns(TEST_NOW_NS);
    assert_eq!(
        {
            set_ctx(voter(), 0, voting_start);
            contract.get_proposal(id).unwrap().proposal.status
        },
        ProposalStatus::Voting
    );

    set_ctx(council(), 1, voting_start);
    contract.veto_proposal(id);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Vetoed
    );
}

#[test]
#[should_panic(expected = "Proposal cannot be vetoed in its current state")]
fn veto_classic_during_voting_panics() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    set_ctx(council(), 1, TEST_NOW_NS);
    contract.veto_proposal(id);
}

// noveto_proposal

#[test]
fn noveto_signaling_only_proposal_succeeds_immediately() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(voter(), 0, voting_end);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Timelock
    );

    set_ctx(council(), 1, voting_end);
    contract.noveto_proposal(id);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Succeeded
    );
}

#[test]
fn noveto_with_actions_moves_to_executable() {
    // With actions, noveto transitions Timelock -> Executable, not -> Succeeded.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();

    set_ctx(
        proposer(),
        NearToken::from_near(200).as_yoctonear(),
        TEST_NOW_NS,
    );
    let id = contract.create_proposal(
        crate::metadata::ProposalMetadata {
            title: Some("noveto-actions".to_string()),
            description: None,
            link: None,
        },
        Some(vec![crate::proposal::ProposalAction::Transfer {
            receiver_id: acc("dest.test.near"),
            amount: NearToken::from_near(1),
        }]),
        ProposalFlow::Classic,
    );
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    contract.proposals.flush();
    near_sdk::testing_env!(
        VMContextBuilder::new()
            .current_account_id(current_account())
            .predecessor_account_id(current_account())
            .attached_deposit(NearToken::from_yoctonear(0))
            .block_timestamp(TEST_NOW_NS)
            .build()
    );
    contract.on_get_snapshot((fixture.snapshot.clone(), fixture.vgs.clone()), id);
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(voter(), 0, voting_end);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Timelock
    );

    set_ctx(council(), 1, voting_end);
    contract.noveto_proposal(id);
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Executable
    );
}

#[test]
#[should_panic(expected = "Proposal can only be noveto'd during the timelock period")]
fn noveto_outside_timelock_panics() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    set_ctx(council(), 1, TEST_NOW_NS);
    contract.noveto_proposal(id);
}

#[test]
#[should_panic(expected = "Only the council can call this method")]
fn noveto_requires_council() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(reviewer(), 1, voting_end);
    contract.noveto_proposal(id);
}

// slash_proposal

#[test]
fn slash_fasttrack_with_zero_bond_returns_value_variant() {
    // Zero bond_amount makes slash return PromiseOrValue::Value(()), not a transfer promise.
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_bond_amount(NearToken::ZERO);

    // Bond is zero, so creation no longer demands it.
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);

    set_ctx(reviewer(), 1, TEST_NOW_NS);
    match contract.slash_proposal(id) {
        PromiseOrValue::Value(()) => {}
        PromiseOrValue::Promise(_) => panic!("expected Value variant for zero-bond slash"),
    }
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Slashed
    );
}

#[test]
fn slash_fasttrack_in_created_moves_to_slashed() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.slash_proposal(id);
    let p = contract.get_proposal(id).unwrap().proposal;
    assert_eq!(p.status, ProposalStatus::Slashed);
    // bond was forfeited to treasury, contract drops it from the proposal.
    assert_eq!(p.bond_amount, NearToken::ZERO);
}

#[test]
#[should_panic(expected = "Only FastTrack proposals can be slashed")]
fn slash_rejects_classic_flow() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.slash_proposal(id);
}

#[test]
#[should_panic(expected = "Proposal can only be slashed while in Created status")]
fn slash_rejects_already_approved_fasttrack() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, id, None);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.slash_proposal(id);
}

#[test]
#[should_panic(expected = "Only the reviewers can call this method")]
fn slash_requires_reviewer() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    set_ctx(council(), 1, TEST_NOW_NS);
    let _ = contract.slash_proposal(id);
}

// on_get_snapshot — wrong-status panic, distinct from votes::tests coverage.

#[test]
#[should_panic(expected = "Proposal must be in Sandbox or Voting status")]
fn on_get_snapshot_panics_for_created_proposal() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    near_sdk::testing_env!(
        VMContextBuilder::new()
            .current_account_id(current_account())
            .predecessor_account_id(current_account())
            .attached_deposit(NearToken::from_yoctonear(0))
            .block_timestamp(TEST_NOW_NS)
            .build()
    );
    contract.on_get_snapshot((fixture.snapshot.clone(), fixture.vgs.clone()), id);
}

// Pause guards: assert_one_yocto() runs before assert_not_paused(), so attach 1 yocto.

#[test]
#[should_panic(expected = "Contract is paused")]
fn approve_panics_when_paused() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    contract.paused = true;
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn reject_panics_when_paused() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    contract.paused = true;
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    contract.reject_proposal(id);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn veto_panics_when_paused() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    contract.paused = true;
    set_ctx(council(), 1, TEST_NOW_NS);
    contract.veto_proposal(id);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn noveto_panics_when_paused() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    contract.paused = true;
    set_ctx(council(), 1, TEST_NOW_NS);
    contract.noveto_proposal(id);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn slash_panics_when_paused() {
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    contract.paused = true;
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.slash_proposal(id);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn on_get_snapshot_panics_when_paused() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    contract.paused = true;
    near_sdk::testing_env!(
        VMContextBuilder::new()
            .current_account_id(current_account())
            .predecessor_account_id(current_account())
            .attached_deposit(NearToken::from_yoctonear(0))
            .block_timestamp(TEST_NOW_NS)
            .build()
    );
    contract.on_get_snapshot((fixture.snapshot.clone(), fixture.vgs.clone()), id);
}
