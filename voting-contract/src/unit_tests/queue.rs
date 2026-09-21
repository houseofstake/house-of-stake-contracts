//! Active-set membership checks also assert stored status via `assert_active_with_status`.
use crate::metadata::ProposalMetadata;
use crate::proposal::ProposalStatus::*;
use crate::proposal::{MajorityType, Proposal, ProposalFlow, ProposalStatus};
use crate::unit_tests::test_utils::*;
use crate::*;
use common::voting::VoteOption;
use near_sdk::json_types::U64;

// Basics

#[test]
fn get_queue_state_on_empty_contract_returns_empty() {
    let contract = fresh_contract();
    let state = contract.get_queue_state();
    assert!(state.active_proposals.is_empty());
    assert!(state.pending_queue.is_empty());
}

#[test]
fn advance_queue_is_a_noop_when_nothing_is_pending() {
    let mut contract = fresh_contract();
    set_ctx(proposer(), 0, TEST_NOW_NS);
    contract.advance_queue();
    let state = contract.get_queue_state();
    assert!(state.active_proposals.is_empty());
    assert!(state.pending_queue.is_empty());
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn advance_queue_when_paused_panics() {
    let mut contract = fresh_contract();
    contract.paused = true;
    set_ctx(proposer(), 0, TEST_NOW_NS);
    contract.advance_queue();
}

#[test]
fn approval_with_full_slots_appends_to_pending_queue() {
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let first = create_proposal(&mut contract, ProposalFlow::Classic);
    let second = create_proposal(&mut contract, ProposalFlow::Classic);
    let third = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, first, None);
    approve_proposal(&mut contract, second, None);
    approve_proposal(&mut contract, third, None);

    assert_active_with_status(&contract, first, ProposalStatus::Voting);
    assert_queued_at(&contract, second, 0);
    assert_queued_at(&contract, third, 1);
}

#[test]
fn active_proposal_drops_off_queue_state_once_voting_ends() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(100))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let id = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, id, Some(&fixture));
    assert_active_with_status(&contract, id, ProposalStatus::Voting);

    // Advance past voting_end without quorum -> Defeated, slot frees.
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(voter(), 0, voting_end);
    let state = contract.get_queue_state();
    assert!(!state.active_proposals.contains(&id));
    assert_eq!(
        contract.get_proposal(id).unwrap().proposal.status,
        ProposalStatus::Defeated
    );
}

#[test]
fn get_queue_state_promotes_queued_proposal_virtually_when_slot_frees() {
    // get_queue_state virtually promotes the queued proposal before advance_queue commits.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(100))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let first = create_proposal(&mut contract, ProposalFlow::Classic);
    let second = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, first, Some(&fixture));
    approve_proposal(&mut contract, second, None);
    assert_active_with_status(&contract, first, ProposalStatus::Voting);
    assert_queued_at(&contract, second, 0);

    // Advance to first proposal's voting_end -> Defeated, slot freed.
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(voter(), 0, voting_end);
    // get_proposal() runs the virtual update too, so the promoted status is Voting.
    assert_eq!(
        contract.get_proposal(first).unwrap().proposal.status,
        ProposalStatus::Defeated
    );
    assert_eq!(
        contract.get_proposal(second).unwrap().proposal.status,
        ProposalStatus::Voting
    );
    let state = contract.get_queue_state();
    assert!(!state.active_proposals.contains(&first));
    assert!(state.active_proposals.contains(&second));
    assert!(state.pending_queue.is_empty());
}

#[test]
fn advance_queue_commits_virtual_promotion_to_storage() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(100))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);
    let first = create_proposal(&mut contract, ProposalFlow::Classic);
    let second = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, first, Some(&fixture));
    approve_proposal(&mut contract, second, None);

    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(proposer(), 0, voting_end);
    contract.advance_queue();

    // Read RAW stored state to verify the promotion landed in storage, not just virtually.
    let raw_first: Proposal = contract.proposals.get(first).cloned().unwrap().into();
    let raw_second: Proposal = contract.proposals.get(second).cloned().unwrap().into();
    assert_eq!(raw_first.status, ProposalStatus::Defeated);
    assert_eq!(raw_second.status, ProposalStatus::Voting);
    // Queue promotion must NOT auto-fetch a snapshot; the first voter takes it.
    assert!(raw_second.snapshot_and_state.is_none());
    assert!(!contract.active_proposals.contains(&first));
    assert!(contract.active_proposals.contains(&second));
    assert!(contract.pending_queue.is_empty());
}

// Default cap

#[test]
fn default_cap_allows_three_concurrent_active_proposals() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    assert_eq!(contract.get_config().max_active_proposals, 3);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, Some(&fixture));
    approve_proposal(&mut contract, c, Some(&fixture));

    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert_active_with_status(&contract, c, ProposalStatus::Voting);
    assert!(contract.get_queue_state().pending_queue.is_empty());
}

// Backdating

#[test]
fn promoted_proposal_backdates_voting_start_to_freed_slot_end_time() {
    // max_active = 1 isolates one slot freeing and one promotion.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, None);
    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_queued_at(&contract, b, 0);

    let cfg = default_config();
    let a_voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
    // One minute past A's voting_end frees the slot while B's window stays future.
    let advance_to = a_voting_end + 60 * 1_000_000_000;
    set_ctx(proposer(), 0, advance_to);
    contract.advance_queue();

    let b_raw: Proposal = contract.proposals.get(b).cloned().unwrap().into();
    assert_eq!(
        b_raw.voting_start_time_ns,
        Some(U64(a_voting_end)),
        "promoted proposal must backdate start to the freed slot's end_time, not to `now`"
    );
    assert_eq!(b_raw.status, ProposalStatus::Voting);
    assert!(
        a_voting_end + cfg.classic_voting_duration_ns.0 > advance_to,
        "fixture invariant: B's voting_end must remain in the future"
    );
}

#[test]
fn backdating_cascade_through_already_stale_windows() {
    // At 2.5x duration, A and B windows are past (Defeated); C's straddles now -> Voting.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let voting_duration = default_config().classic_voting_duration_ns.0;
    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, None);
    approve_proposal(&mut contract, c, None);
    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_queued_at(&contract, b, 0);
    assert_queued_at(&contract, c, 1);

    let advance_to = TEST_NOW_NS + voting_duration * 5 / 2;
    set_ctx(proposer(), 0, advance_to);
    contract.advance_queue();

    let a_raw: Proposal = contract.proposals.get(a).cloned().unwrap().into();
    let b_raw: Proposal = contract.proposals.get(b).cloned().unwrap().into();
    let c_raw: Proposal = contract.proposals.get(c).cloned().unwrap().into();

    assert_eq!(a_raw.status, ProposalStatus::Defeated);
    assert_eq!(b_raw.status, ProposalStatus::Defeated);
    assert_eq!(c_raw.status, ProposalStatus::Voting);

    assert_eq!(
        b_raw.voting_start_time_ns,
        Some(U64(TEST_NOW_NS + voting_duration))
    );
    assert_eq!(
        c_raw.voting_start_time_ns,
        Some(U64(TEST_NOW_NS + 2 * voting_duration))
    );

    // Pending queue is fully drained.
    assert!(contract.pending_queue.is_empty());
    // Only C is active.
    assert!(!contract.active_proposals.contains(&a));
    assert!(!contract.active_proposals.contains(&b));
    assert!(contract.active_proposals.contains(&c));
}

// FIFO and partial promotion

#[test]
fn slot_frees_one_at_a_time_promotes_head_only() {
    // max_active = 2, staggered creation: one slot frees, head promotes, tail stays.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(2);

    let cfg = default_config();

    let a = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, a, Some(&fixture));
    // Stagger by 1 hour so `between` lands cleanly inside [a_end, b_end].
    let stagger_ns: u64 = 3_600 * 1_000_000_000;
    let b_creation = TEST_NOW_NS + stagger_ns;
    set_ctx(
        proposer(),
        NearToken::from_near(100).as_yoctonear(),
        b_creation,
    );
    let b = contract.create_proposal(
        ProposalMetadata {
            title: Some("b".to_string()),
            description: None,
            link: None,
        },
        None,
        ProposalFlow::Classic,
    );
    set_ctx(reviewer(), 1, b_creation);
    let _ = contract.approve_proposal(b, MajorityType::Simple);
    contract.proposals.flush();
    near_sdk::testing_env!(
        VMContextBuilder::new()
            .current_account_id(current_account())
            .predecessor_account_id(current_account())
            .attached_deposit(NearToken::from_yoctonear(0))
            .block_timestamp(b_creation)
            .build()
    );
    contract.on_get_snapshot((fixture.snapshot.clone(), fixture.vgs.clone()), b);

    let c = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, c, None);
    let d = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, d, None);

    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert_queued_at(&contract, c, 0);
    assert_queued_at(&contract, d, 1);

    // Only A's voting_end has passed: A Defeated, slot frees, one queued promotes.
    let a_voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
    let between = a_voting_end + 60 * 1_000_000_000;
    let b_voting_end = b_creation + cfg.classic_voting_duration_ns.0;
    assert!(
        between < b_voting_end,
        "fixture invariant broken: between={} b_end={}",
        between,
        b_voting_end
    );

    set_ctx(proposer(), 0, between);
    contract.advance_queue();

    assert_eq!(
        contract.get_proposal(a).unwrap().proposal.status,
        ProposalStatus::Defeated
    );
    assert_eq!(
        contract.get_proposal(b).unwrap().proposal.status,
        ProposalStatus::Voting
    );
    assert_eq!(
        contract.get_proposal(c).unwrap().proposal.status,
        ProposalStatus::Voting
    );
    // D stays queued and shifts to head.
    assert_queued_at(&contract, d, 0);
}

// Mixed flow types

#[test]
fn mixed_classic_and_fasttrack_share_active_slots() {
    // Cap 3: Classic Voting, FastTrack Sandbox, FastTrack-threshold-met -> Scheduled.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();

    let classic = create_proposal(&mut contract, ProposalFlow::Classic);
    let sandbox_id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    let scheduled_id = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, classic, Some(&fixture));
    approve_proposal(&mut contract, sandbox_id, Some(&fixture));
    approve_proposal(&mut contract, scheduled_id, Some(&fixture));
    // A For vote (400 NEAR clears the 10% threshold) graduates it to Scheduled.
    let (proof, v_account) = fixture.proof_for(&for_voter());
    set_ctx(
        for_voter(),
        NearToken::from_millinear(10).as_yoctonear(),
        TEST_NOW_NS,
    );
    contract.vote(scheduled_id, VoteOption::For, proof, v_account);

    assert_active_with_status(&contract, classic, ProposalStatus::Voting);
    assert_active_with_status(&contract, sandbox_id, ProposalStatus::Sandbox);
    assert_active_with_status(&contract, scheduled_id, ProposalStatus::Scheduled);
    assert_eq!(contract.get_queue_state().active_proposals.len(), 3);
}

// max_active_proposals changes

#[test]
fn reducing_max_active_does_not_evict_existing_active_proposals() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();

    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, Some(&fixture));
    approve_proposal(&mut contract, c, Some(&fixture));

    // Shrink cap below current load.
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert_active_with_status(&contract, c, ProposalStatus::Voting);

    // Subsequent approvals must queue, since virtual_active_count = 3 >= cap = 1.
    let d = create_proposal(&mut contract, ProposalFlow::Classic);
    let e = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, d, None);
    approve_proposal(&mut contract, e, None);
    assert_queued_at(&contract, d, 0);
    assert_queued_at(&contract, e, 1);

    // 3 slots free but cap = 1, so only one queued promotes; the other stays at pos 0.
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
    set_ctx(proposer(), 0, voting_end);
    contract.advance_queue();

    assert_active_with_status(&contract, d, ProposalStatus::Voting);
    assert_queued_at(&contract, e, 0);
    assert_eq!(contract.active_proposals.len(), 1);
}

#[test]
fn lifting_max_active_promotes_multiple_queued_at_once() {
    // Lifting cap 1 -> 3 promotes B and C into the new slots; D stays queued.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    let d = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, None);
    approve_proposal(&mut contract, c, None);
    approve_proposal(&mut contract, d, None);
    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_queued_at(&contract, b, 0);
    assert_queued_at(&contract, c, 1);
    assert_queued_at(&contract, d, 2);

    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(3);

    assert_active_with_status(&contract, a, ProposalStatus::Voting);
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert_active_with_status(&contract, c, ProposalStatus::Voting);
    assert_queued_at(&contract, d, 0);
}

// Slot-freeing exits: veto / noveto / sandbox-timeout

#[test]
fn fasttrack_sandbox_timeout_frees_slot_for_queued_promotion() {
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    // 50 NEAR is below the 10% sandbox threshold, so A times out as Defeated.
    let a_fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(50))],
        NearToken::from_near(1_000),
    );
    let a = create_proposal(&mut contract, ProposalFlow::FastTrack);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&a_fixture));
    approve_proposal(&mut contract, b, None);
    assert_active_with_status(&contract, a, ProposalStatus::Sandbox);
    assert_queued_at(&contract, b, 0);

    // Sandbox window expires.
    let sandbox_end = TEST_NOW_NS + default_config().sandbox_duration_ns.0;
    set_ctx(proposer(), 0, sandbox_end);
    contract.advance_queue();

    assert_eq!(
        contract.get_proposal(a).unwrap().proposal.status,
        ProposalStatus::Defeated
    );
    // B has been promoted with backdated start = a.sandbox_end.
    let b_raw: Proposal = contract.proposals.get(b).cloned().unwrap().into();
    assert_eq!(b_raw.voting_start_time_ns, Some(U64(sandbox_end)));
    // sandbox_end + classic_voting_duration is in the future of `now`, so B is Voting.
    assert_eq!(b_raw.status, ProposalStatus::Voting);
}

#[test]
fn council_veto_during_timelock_frees_slot_for_queued_promotion() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, a, Some(&fixture));
    let (proof, v_account) = fixture.proof_for(&for_voter());
    set_ctx(
        for_voter(),
        NearToken::from_millinear(10).as_yoctonear(),
        TEST_NOW_NS,
    );
    contract.vote(a, VoteOption::For, proof, v_account);

    let b = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, b, None);
    assert_queued_at(&contract, b, 0);

    let cfg = default_config();
    let voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
    // Move A into Timelock then veto it.
    assert_eq!(
        {
            set_ctx(voter(), 0, voting_end);
            contract.get_proposal(a).unwrap().proposal.status
        },
        ProposalStatus::Timelock
    );

    set_ctx(council(), 1, voting_end);
    contract.veto_proposal(a);

    // veto_proposal triggers internal_advance_queue, so B promotes.
    assert_eq!(
        contract.get_proposal(a).unwrap().proposal.status,
        ProposalStatus::Vetoed
    );
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert!(contract.pending_queue.is_empty());
}

#[test]
fn council_noveto_during_timelock_frees_slot_for_queued_promotion() {
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(1);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, a, Some(&fixture));
    let (proof, v_account) = fixture.proof_for(&for_voter());
    set_ctx(
        for_voter(),
        NearToken::from_millinear(10).as_yoctonear(),
        TEST_NOW_NS,
    );
    contract.vote(a, VoteOption::For, proof, v_account);

    let b = create_proposal(&mut contract, ProposalFlow::Classic);

    approve_proposal(&mut contract, b, None);
    assert_queued_at(&contract, b, 0);

    let cfg = default_config();
    let voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
    // Move A into Timelock then noveto it (slot would otherwise hold until timelock end).
    assert_eq!(
        {
            set_ctx(voter(), 0, voting_end);
            contract.get_proposal(a).unwrap().proposal.status
        },
        ProposalStatus::Timelock
    );

    set_ctx(council(), 1, voting_end);
    contract.noveto_proposal(a);

    // noveto_proposal triggers internal_advance_queue, so B promotes; A has no actions -> Succeeded.
    assert_eq!(
        contract.get_proposal(a).unwrap().proposal.status,
        ProposalStatus::Succeeded
    );
    assert_active_with_status(&contract, b, ProposalStatus::Voting);
    assert!(contract.pending_queue.is_empty());
}

// Lifecycle walkthrough

#[test]
fn virtual_queue_walkthrough_across_all_lifecycle_paths() {
    // Slot-freeing across three active lifecycle paths with seven queued proposals.
    let fixture = snapshot_with_voters(
        &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();

    // Default cap is 3 — exactly room for A, B, C.
    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    cast_vote_at(
        &mut contract,
        &fixture,
        for_voter(),
        a,
        VoteOption::For,
        TEST_NOW_NS,
    );

    let b = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, b, Some(&fixture));

    let c = create_proposal(&mut contract, ProposalFlow::FastTrack);
    approve_proposal(&mut contract, c, Some(&fixture));
    cast_vote_at(
        &mut contract,
        &fixture,
        for_voter(),
        c,
        VoteOption::For,
        TEST_NOW_NS,
    );

    let mut queued = Vec::with_capacity(7);
    for i in 0..7 {
        let flow = if i % 2 == 0 {
            ProposalFlow::Classic
        } else {
            ProposalFlow::FastTrack
        };
        let id = create_proposal(&mut contract, flow);
        approve_proposal(&mut contract, id, None);
        queued.push(id);
    }
    let cfg = default_config();
    let t_c_voting_start = crate::proposal::next_voting_start_ns(TEST_NOW_NS);
    let t_b_sandbox_end = TEST_NOW_NS + cfg.sandbox_duration_ns.0;
    let t_c_voting_end = t_c_voting_start + cfg.fast_track_voting_duration_ns.0;
    let t_a_voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
    let t_a_timelock_end = t_a_voting_end + cfg.timelock_duration_ns.0;
    // Pin the only non-derived fact: next_voting_start_ns rounds to Monday 2026-06-08.
    assert_eq!(t_c_voting_start, date_ns(2026, 6, 8));

    // T0 — A Voting, B Sandbox, C Scheduled (vote pushed past threshold).
    assert_eq!(
        all_statuses_at(&contract, TEST_NOW_NS),
        vec![
            Voting, Sandbox, Scheduled, Queued, Queued, Queued, Queued, Queued, Queued, Queued
        ],
    );

    // C Scheduled → Voting at next-Monday start (slot still held).
    assert_eq!(
        all_statuses_at(&contract, t_c_voting_start),
        vec![
            Voting, Sandbox, Voting, Queued, Queued, Queued, Queued, Queued, Queued, Queued
        ],
    );

    // CHECKPOINT 1 — B Defeated, Q1 (Classic) promoted backdated to B's sandbox_end.
    assert_eq!(
        all_statuses_at(&contract, t_b_sandbox_end),
        vec![
            Voting, Defeated, Voting, Voting, Queued, Queued, Queued, Queued, Queued, Queued
        ],
    );
    assert_eq!(
        contract
            .get_proposal(queued[0])
            .unwrap()
            .proposal
            .voting_start_time_ns,
        Some(U64(t_b_sandbox_end)),
        "Q1 backdates voting_start to B's sandbox_end"
    );

    // CHECKPOINT 2 — C Succeeded, Q2 (FastTrack) promoted backdated to C's voting_end.
    assert_eq!(
        all_statuses_at(&contract, t_c_voting_end),
        vec![
            Voting, Defeated, Succeeded, Voting, Sandbox, Queued, Queued, Queued, Queued, Queued
        ],
    );
    assert_eq!(
        contract
            .get_proposal(queued[1])
            .unwrap()
            .proposal
            .sandbox_start_time_ns,
        Some(U64(t_c_voting_end)),
        "Q2 backdates sandbox_start to C's voting_end"
    );

    // A Voting → Timelock (still active; slot held).
    assert_eq!(
        all_statuses_at(&contract, t_a_voting_end),
        vec![
            Timelock, Defeated, Succeeded, Voting, Sandbox, Queued, Queued, Queued, Queued, Queued
        ],
    );

    // CHECKPOINT 3 — A Succeeded.
    assert_eq!(
        all_statuses_at(&contract, t_a_timelock_end),
        vec![
            Succeeded, Defeated, Succeeded, Defeated, Defeated, Voting, Defeated, Voting, Sandbox,
            Queued
        ],
    );
}

// Ordering invariants

#[test]
fn active_proposals_iterate_in_approval_order() {
    // get_queue_state lists active_proposals in approval order.
    let fixture = snapshot_with_voters(
        &[
            VoterSpec::new(for_voter(), NearToken::from_near(400)),
            VoterSpec::new(against_voter(), NearToken::from_near(100)),
            VoterSpec::new(abstain_voter(), NearToken::from_near(50)),
        ],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, Some(&fixture));
    approve_proposal(&mut contract, c, Some(&fixture));

    let state = contract.get_queue_state();
    assert_eq!(
        state.active_proposals,
        vec![a, b, c],
        "active_proposals must iterate in approval order"
    );
}

#[test]
fn active_proposals_ordering_is_set_membership_only_after_removals() {
    let fixture = snapshot_with_voters(
        &[
            VoterSpec::new(for_voter(), NearToken::from_near(400)),
            VoterSpec::new(against_voter(), NearToken::from_near(100)),
            VoterSpec::new(abstain_voter(), NearToken::from_near(50)),
        ],
        NearToken::from_near(1_000),
    );
    let mut contract = fresh_contract();
    set_ctx(owner(), 1, TEST_NOW_NS);
    contract.set_max_active_proposals(2);

    let a = create_proposal(&mut contract, ProposalFlow::Classic);
    let b = create_proposal(&mut contract, ProposalFlow::Classic);
    let c = create_proposal(&mut contract, ProposalFlow::Classic);
    let d = create_proposal(&mut contract, ProposalFlow::Classic);
    approve_proposal(&mut contract, a, Some(&fixture));
    approve_proposal(&mut contract, b, Some(&fixture));
    approve_proposal(&mut contract, c, None);
    approve_proposal(&mut contract, d, None);

    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0 + 1;
    set_ctx(voter(), 0, voting_end);
    let mut before_active = contract.get_queue_state().active_proposals;
    before_active.sort();

    set_ctx(proposer(), 0, voting_end);
    contract.advance_queue();

    let mut after_active = contract.get_queue_state().active_proposals;
    after_active.sort();

    let mut expected = vec![c, d];
    expected.sort();
    assert_eq!(before_active, expected, "virtual must promote C and D");
    assert_eq!(after_active, expected, "committed must contain C and D");
    let _ = a;
    let _ = b;
}

// Virtual-vs-committed equivalence

// Compares a read-only (virtual) contract against one committed via advance_queue.

#[test]
fn virtual_matches_committed_in_backdating_cascade() {
    let fixture = snapshot_with_voters(
        &[
            VoterSpec::new(for_voter(), NearToken::from_near(400)),
            VoterSpec::new(against_voter(), NearToken::from_near(100)),
            VoterSpec::new(abstain_voter(), NearToken::from_near(50)),
        ],
        NearToken::from_near(1_000),
    );

    let virtual_c = {
        let mut c = fresh_contract();
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        c
    };
    let mut committed_c = {
        let mut c = fresh_contract();
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, None);
        c
    };

    let voting_duration = default_config().classic_voting_duration_ns.0;
    let advance_to = TEST_NOW_NS + voting_duration * 5 / 2;

    set_ctx(voter(), 0, advance_to);
    let v_state = virtual_c.get_queue_state();
    let v_statuses: Vec<_> = (0..virtual_c.get_num_proposals())
        .map(|i| virtual_c.get_proposal(i).unwrap().proposal.status)
        .collect();

    set_ctx(proposer(), 0, advance_to);
    committed_c.advance_queue();
    set_ctx(voter(), 0, advance_to);
    let c_state = committed_c.get_queue_state();
    let c_statuses: Vec<_> = (0..committed_c.get_num_proposals())
        .map(|i| committed_c.get_proposal(i).unwrap().proposal.status)
        .collect();

    let mut v_active = v_state.active_proposals.clone();
    let mut c_active = c_state.active_proposals.clone();
    v_active.sort();
    c_active.sort();
    assert_eq!(v_active, c_active);
    assert_eq!(v_state.pending_queue, c_state.pending_queue);
    assert_eq!(v_statuses, c_statuses);
}

#[test]
fn virtual_matches_committed_when_active_proposals_transition_without_queue_promotion() {
    // Three active, no queue: all transition Voting -> Defeated via active_updates, not promotions.
    let fixture = snapshot_with_voters(
        &[
            VoterSpec::new(for_voter(), NearToken::from_near(400)),
            VoterSpec::new(against_voter(), NearToken::from_near(100)),
            VoterSpec::new(abstain_voter(), NearToken::from_near(50)),
        ],
        NearToken::from_near(1_000),
    );

    let virtual_c = {
        let mut c = fresh_contract();
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        c
    };
    let mut committed_c = {
        let mut c = fresh_contract();
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        let id = create_proposal(&mut c, ProposalFlow::Classic);
        approve_proposal(&mut c, id, Some(&fixture));
        c
    };

    // No votes cast -> below quorum -> all three Defeated at voting_end.
    let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;

    set_ctx(voter(), 0, voting_end);
    let v_state = virtual_c.get_queue_state();
    let v_statuses: Vec<_> = (0..virtual_c.get_num_proposals())
        .map(|i| virtual_c.get_proposal(i).unwrap().proposal.status)
        .collect();

    set_ctx(proposer(), 0, voting_end);
    committed_c.advance_queue();
    set_ctx(voter(), 0, voting_end);
    let c_state = committed_c.get_queue_state();
    let c_statuses: Vec<_> = (0..committed_c.get_num_proposals())
        .map(|i| committed_c.get_proposal(i).unwrap().proposal.status)
        .collect();

    let mut v_active = v_state.active_proposals.clone();
    let mut c_active = c_state.active_proposals.clone();
    v_active.sort();
    c_active.sort();
    assert_eq!(v_active, c_active);
    assert_eq!(v_state.pending_queue, c_state.pending_queue);
    assert_eq!(v_statuses, c_statuses);
}
