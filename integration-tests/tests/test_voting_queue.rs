mod setup;

use crate::setup::VenearTestWorkspaceBuilder;
use crate::setup::voting_helpers::*;
use common::voting::{MajorityType, ProposalStatus, VoteOption};
use near_sdk::{Gas, NearToken};
use serde_json::json;

/// Vetoing a Scheduled/Timelock proposal frees a slot — `reject_proposal`'s auto-tick promotes
/// the next queued proposal. Exercises Classic→Voting and FastTrack→Sandbox promotion paths.
#[tokio::test]
async fn test_queued_promotes_on_veto() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;
    let council = &v.voting.as_ref().unwrap().council;

    let mut ids = Vec::new();
    for _ in 0..4 {
        let id = create_proposal_fst(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        ids.push(id);
    }
    let classic_p = create_proposal(&v, &user, None).await?;
    approve_proposal(&v, reviewer, classic_p, MajorityType::Simple).await?;

    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Queued
    );
    assert_eq!(
        get_status(&v.get_proposal(classic_p).await?)?,
        ProposalStatus::Queued
    );
    assert_eq!(
        get_queue_state(&v).await?.pending_queue,
        vec![ids[3], classic_p]
    );

    vote_for_option(&v, &user, ids[0], VoteOption::For).await?;
    assert_eq!(
        get_status(&v.get_proposal(ids[0]).await?)?,
        ProposalStatus::Scheduled
    );
    veto_proposal(&v, council, ids[0]).await?.into_result()?;

    assert_eq!(
        get_status(&v.get_proposal(ids[0]).await?)?,
        ProposalStatus::Vetoed
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Sandbox,
        "FastTrack queued proposal should be promoted into Sandbox after a slot freed via veto"
    );
    assert_eq!(
        get_status(&v.get_proposal(classic_p).await?)?,
        ProposalStatus::Queued,
        "Classic still waiting — only one slot freed"
    );
    vote_for_option(&v, &user, ids[1], VoteOption::For).await?;
    assert_eq!(
        get_status(&v.get_proposal(ids[1]).await?)?,
        ProposalStatus::Scheduled
    );
    veto_proposal(&v, council, ids[1]).await?.into_result()?;

    assert_eq!(
        get_status(&v.get_proposal(ids[1]).await?)?,
        ProposalStatus::Vetoed
    );
    assert_eq!(
        get_status(&v.get_proposal(classic_p).await?)?,
        ProposalStatus::Voting,
        "Classic queued proposal should be promoted into Voting after a slot freed via veto"
    );
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);
    assert_eq!(get_queue_state(&v).await?.pending_queue.len() as u32, 0);

    Ok(())
}

#[tokio::test]
async fn test_queued_promotes_on_sandbox_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let mut ids = Vec::new();
    for _ in 0..4 {
        let id = create_proposal_fst(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        ids.push(id);
    }
    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Queued
    );

    let p0_pre = v.get_proposal(ids[0]).await?;
    let sandbox_duration: u64 = p0_pre["sandbox_duration_ns"].as_str().unwrap().parse()?;
    let p0_sandbox_start: u64 = p0_pre["sandbox_start_time_ns"].as_str().unwrap().parse()?;
    let earliest_freed_slot_end = p0_sandbox_start + sandbox_duration;

    // Fast-forward past the sandbox deadline. The three already-Sandbox proposals all time out.
    v.fast_forward_to_proposal_status_fst(ids[0], ProposalStatus::Defeated)
        .await?;

    assert_eq!(
        get_status(&v.get_proposal(ids[0]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[1]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[2]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Sandbox,
        "View should already show ids[3] as virtually promoted before advance_queue is called"
    );
    let pre = get_queue_state(&v).await?;
    assert_eq!(pre.active_proposals, vec![ids[3]]);
    assert_eq!(pre.pending_queue.len(), 0);

    // Now actually persist the virtual state via an explicit advance_queue. Anyone can call it.
    let outcome = advance_queue(&v, &user).await?;
    assert!(outcome.is_success(), "advance_queue failed: {:#?}", outcome);

    assert_eq!(
        get_status(&v.get_proposal(ids[0]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[1]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[2]).await?)?,
        ProposalStatus::Defeated
    );
    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Sandbox,
        "Queued proposal should be promoted — its sandbox clock starts at admission, not earlier"
    );
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 1);
    assert_eq!(get_queue_state(&v).await?.pending_queue.len() as u32, 0);

    // Backdating: ids[3] inherits ids[0]'s sandbox_end (earliest freed slot), not `now`.
    // A regression that switches to `now` would silently shift every promoted deadline forward.
    let p3_promoted = v.get_proposal(ids[3]).await?;
    let p3_sandbox_start: u64 = p3_promoted["sandbox_start_time_ns"]
        .as_str()
        .unwrap()
        .parse()?;
    assert_eq!(
        p3_sandbox_start, earliest_freed_slot_end,
        "Promoted proposal must inherit the earliest freed slot's end_time as its start"
    );

    Ok(())
}

/// Classic proposals in Timelock continue to hold an active slot. Filling all slots with
/// Timelocks must force a newly approved Classic proposal into the queue.
#[tokio::test]
async fn test_classic_timelock_holds_slot() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let mut ids = Vec::new();
    for _ in 0..3 {
        let id = create_proposal(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        ids.push(id);
    }
    for &id in &ids {
        assert_eq!(
            get_status(&v.get_proposal(id).await?)?,
            ProposalStatus::Voting
        );
    }
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);

    // Vote For so each proposal transitions to Timelock (rather than Defeated) at voting end.
    for &id in &ids {
        vote_for_option(&v, &user, id, VoteOption::For).await?;
    }

    v.fast_forward_to_proposal_status(ids[2], ProposalStatus::Timelock)
        .await?;
    for &id in &ids {
        assert_eq!(
            get_status(&v.get_proposal(id).await?)?,
            ProposalStatus::Timelock
        );
    }
    assert_eq!(
        get_queue_state(&v).await?.active_proposals.len() as u32,
        3,
        "Timelock proposals must still occupy active slots"
    );

    // Approve a fourth Classic proposal. Cap is full of Timelocks — it must queue.
    let id4 = create_proposal(&v, &user, None).await?;
    approve_proposal(&v, reviewer, id4, MajorityType::Simple).await?;

    assert_eq!(
        get_status(&v.get_proposal(id4).await?)?,
        ProposalStatus::Queued,
        "4th Classic must queue because Timelock holds a slot"
    );
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);
    assert_eq!(get_queue_state(&v).await?.pending_queue, vec![id4]);

    Ok(())
}

#[tokio::test]
async fn test_classic_mixed_lifecycle_then_more_approvals() -> Result<(), Box<dyn std::error::Error>>
{
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let mut ids = Vec::new();
    for _ in 0..6 {
        let id = create_proposal(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        ids.push(id);
    }
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);
    assert_eq!(get_queue_state(&v).await?.pending_queue.len() as u32, 3);

    // Vote For on ids[0] (will pass quorum + threshold). ids[1] and ids[2] get no votes.
    vote_for_option(&v, &user, ids[0], VoteOption::For).await?;
    v.fast_forward_to_proposal_status(ids[0], ProposalStatus::Succeeded)
        .await?;
    v.fast_forward_to_proposal_status(ids[5], ProposalStatus::Defeated)
        .await?;

    assert_eq!(
        get_status(&v.get_proposal(ids[0]).await?)?,
        ProposalStatus::Succeeded
    );
    for i in 1..6 {
        assert_eq!(
            get_status(&v.get_proposal(ids[i]).await?)?,
            ProposalStatus::Defeated,
            "ids[{i}] should be Defeated"
        );
    }

    let mut new_ids = Vec::new();
    for _ in 0..4 {
        let id = create_proposal(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        new_ids.push(id);
    }

    for i in 0..3 {
        assert_eq!(
            get_status(&v.get_proposal(new_ids[i]).await?)?,
            ProposalStatus::Voting,
            "new_ids[{i}] should be Voting"
        );
    }
    assert_eq!(
        get_status(&v.get_proposal(new_ids[3]).await?)?,
        ProposalStatus::Queued
    );

    let state = get_queue_state(&v).await?;
    assert_eq!(state.active_proposals.len(), 3);
    assert_eq!(state.pending_queue, vec![new_ids[3]]);

    Ok(())
}

#[tokio::test]
async fn test_max_active_proposals_setter() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .max_active_proposals(1)
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;

    let owner = &v.voting.as_ref().unwrap().owner;
    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    // With cap=1, two approved FastTrack proposals: first active, second queued.
    let p1 = create_proposal_fst(&v, &user, None).await?;
    let p2 = create_proposal_fst(&v, &user, None).await?;

    approve_proposal(&v, reviewer, p1, MajorityType::Simple).await?;
    approve_proposal(&v, reviewer, p2, MajorityType::Simple).await?;

    assert_eq!(
        get_status(&v.get_proposal(p1).await?)?,
        ProposalStatus::Sandbox
    );
    assert_eq!(
        get_status(&v.get_proposal(p2).await?)?,
        ProposalStatus::Queued
    );

    // Owner raises the cap. The setter persists the queue advance internally, so p2 is
    // promoted by the same call — no explicit advance_queue is needed.
    let outcome = owner
        .call(v.voting_id(), "set_max_active_proposals")
        .args_json(json!({ "max_active_proposals": 2 }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(outcome.is_success(), "owner setter failed: {:#?}", outcome);

    assert_eq!(
        get_status(&v.get_proposal(p2).await?)?,
        ProposalStatus::Sandbox,
        "p2 should be promoted by the setter's internal advance_queue"
    );
    let post = get_queue_state(&v).await?;
    assert_eq!(post.active_proposals, vec![p1, p2]);
    assert_eq!(post.pending_queue.len(), 0);

    // Lower the cap back to 1 while both p1 and p2 are still active.
    let outcome = owner
        .call(v.voting_id(), "set_max_active_proposals")
        .args_json(json!({ "max_active_proposals": 1 }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(outcome.is_success(), "owner setter failed: {:#?}", outcome);

    // Actives unchanged by the cap drop.
    assert_eq!(
        get_queue_state(&v).await?.active_proposals,
        vec![p1, p2],
        "Lowering the cap must not demote active proposals"
    );

    // Approve a third proposal while over-cap. It must queue.
    let p3 = create_proposal_fst(&v, &user, None).await?;
    approve_proposal(&v, reviewer, p3, MajorityType::Simple).await?;
    assert_eq!(
        get_status(&v.get_proposal(p3).await?)?,
        ProposalStatus::Queued,
        "p3 should queue because virtual_active_count (2) >= cap (1)"
    );

    let outcome = advance_queue(&v, &user).await?;
    assert!(outcome.is_success(), "advance_queue failed: {:#?}", outcome);
    assert_eq!(
        get_status(&v.get_proposal(p3).await?)?,
        ProposalStatus::Queued,
        "p3 must remain queued while cap is exceeded by existing actives"
    );
    let post = get_queue_state(&v).await?;
    assert_eq!(post.active_proposals, vec![p1, p2]);
    assert_eq!(post.pending_queue, vec![p3]);

    Ok(())
}

#[tokio::test]
async fn test_view_virtual_advance() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let mut ids = Vec::new();
    for _ in 0..6 {
        let id = create_proposal_fst(&v, &user, None).await?;
        approve_proposal(&v, reviewer, id, MajorityType::Simple).await?;
        ids.push(id);
    }

    // Phase 1: no transitions yet — view shows the stored Queued status.
    assert_eq!(
        get_status(&v.get_proposal(ids[3]).await?)?,
        ProposalStatus::Queued,
        "No slots are free — view must still show Queued"
    );
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);
    assert_eq!(get_queue_state(&v).await?.pending_queue.len() as u32, 3);

    // Phase 2: first sandbox batch expires. ids[0..=2] virtually Defeated,
    // ids[3..=5] virtually promoted into Sandbox.
    v.fast_forward_to_proposal_status_fst(ids[0], ProposalStatus::Defeated)
        .await?;
    for i in 0..3 {
        assert_eq!(
            get_status(&v.get_proposal(ids[i]).await?)?,
            ProposalStatus::Defeated,
            "ids[{i}] should be virtually Defeated"
        );
    }
    for i in 3..6 {
        assert_eq!(
            get_status(&v.get_proposal(ids[i]).await?)?,
            ProposalStatus::Sandbox,
            "ids[{i}] should be virtually promoted to Sandbox"
        );
    }
    assert_eq!(get_queue_state(&v).await?.active_proposals.len() as u32, 3);

    // freed_slot_times priority ordering: end_times are sorted earliest-first; queued proposals
    // pop in FIFO order, so each ids[3+i] inherits ids[i]'s sandbox_end.
    let sandbox_duration: u64 = v.get_proposal(ids[0]).await?["sandbox_duration_ns"]
        .as_str()
        .unwrap()
        .parse()?;
    for i in 0..3 {
        let active_sandbox_start: u64 = v.get_proposal(ids[i]).await?["sandbox_start_time_ns"]
            .as_str()
            .unwrap()
            .parse()?;
        let promoted_sandbox_start: u64 =
            v.get_proposal(ids[3 + i]).await?["sandbox_start_time_ns"]
                .as_str()
                .unwrap()
                .parse()?;
        assert_eq!(
            promoted_sandbox_start,
            active_sandbox_start + sandbox_duration,
            "ids[{}] should inherit ids[{}]'s sandbox_end as its sandbox_start_time",
            3 + i,
            i
        );
    }

    // Phase 3: fast-forward past the *promoted* batch's sandbox window
    v.fast_forward_to_proposal_status_fst(ids[5], ProposalStatus::Defeated)
        .await?;

    for i in 0..6 {
        assert_eq!(
            get_status(&v.get_proposal(ids[i]).await?)?,
            ProposalStatus::Defeated,
            "ids[{i}] should be Defeated"
        );
    }
    let state = get_queue_state(&v).await?;
    assert_eq!(state.active_proposals.len(), 0);
    assert_eq!(state.pending_queue.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_vote_on_promoted_queued_without_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .max_active_proposals(1)
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let p1 = create_proposal_fst(&v, &user, None).await?;
    let p2 = create_proposal_fst(&v, &user, None).await?;

    approve_proposal(&v, reviewer, p1, MajorityType::Simple).await?;
    approve_proposal(&v, reviewer, p2, MajorityType::Simple).await?;

    assert_eq!(
        get_status(&v.get_proposal(p1).await?)?,
        ProposalStatus::Sandbox
    );
    assert_eq!(
        get_status(&v.get_proposal(p2).await?)?,
        ProposalStatus::Queued
    );

    // Time out p1 so the queue advances p2 into Sandbox virtually.
    v.fast_forward_to_proposal_status_fst(p1, ProposalStatus::Defeated)
        .await?;

    let view = v.get_proposal(p2).await?;
    assert_eq!(
        get_status(&view)?,
        ProposalStatus::Sandbox,
        "View should show p2 as Sandbox via virtual promotion"
    );
    assert!(
        view["snapshot_and_state"].is_null(),
        "Promoted-without-snapshot proposal must not have a snapshot yet"
    );

    // vote() runs internal_advance_queue and then panics on the missing snapshot.
    let (merkle_proof, v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;
    let outcome = user
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": p2,
            "vote": VoteOption::For,
            "merkle_proof": merkle_proof,
            "v_account": v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Vote should fail before take_snapshot: {:#?}",
        outcome
    );
    assert!(
        format!("{:?}", outcome).contains("Snapshot has not been taken yet"),
        "Expected snapshot-missing panic, got: {:#?}",
        outcome
    );

    // Now fetch the snapshot (no vote) and confirm it's persisted.
    // Snapshot-only calls must not attach a deposit.
    let take = user
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({ "proposal_id": p2, "vote": serde_json::Value::Null }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        take.is_success(),
        "take_snapshot_and_vote failed: {:#?}",
        take
    );

    let view = v.get_proposal(p2).await?;
    assert!(
        !view["snapshot_and_state"].is_null(),
        "Snapshot should be set after take_snapshot_and_vote"
    );

    // Second snapshot-only call must reject because snapshot is already set.
    let again = user
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({ "proposal_id": p2, "vote": serde_json::Value::Null }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(again.is_failure(), "Second snapshot call must fail");
    assert!(
        format!("{:?}", again).contains("Snapshot is already set for this proposal"),
        "Expected already-set panic, got: {:#?}",
        again
    );

    // Wrong-status check: p1 is Defeated.
    let on_defeated = user
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({ "proposal_id": p1, "vote": serde_json::Value::Null }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(on_defeated.is_failure(), "Call on Defeated must fail");
    assert!(
        format!("{:?}", on_defeated)
            .contains("Proposal must be in Sandbox or Voting status to take a snapshot"),
        "Expected wrong-status panic, got: {:#?}",
        on_defeated
    );

    // Voting now succeeds via the standalone vote() function.
    vote_for_option(&v, &user, p2, VoteOption::For).await?;

    Ok(())
}

#[tokio::test]
async fn test_take_snapshot_and_vote_chained() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .max_active_proposals(1)
        .build()
        .await?;
    let user_a = v.create_account_with_lockup().await?;
    let user_b = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user_a, NearToken::from_near(1000))
        .await?;
    v.transfer_and_lock(&user_b, NearToken::from_near(5000))
        .await?;

    let reviewer = &v.voting.as_ref().unwrap().reviewer;

    let p1 = create_proposal_fst(&v, &user_a, None).await?;
    let p2 = create_proposal_fst(&v, &user_a, None).await?;

    approve_proposal(&v, reviewer, p1, MajorityType::Simple).await?;
    approve_proposal(&v, reviewer, p2, MajorityType::Simple).await?;

    assert_eq!(
        get_status(&v.get_proposal(p2).await?)?,
        ProposalStatus::Queued
    );

    v.fast_forward_to_proposal_status_fst(p1, ProposalStatus::Defeated)
        .await?;

    let (merkle_proof, v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({ "account_id": user_a.id() }))
        .await?
        .json()?;

    let outcome = user_a
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({
            "proposal_id": p2,
            "vote": {
                "vote": VoteOption::For,
                "merkle_proof": merkle_proof,
                "v_account": v_account,
            },
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(150))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Chained take_snapshot_and_vote failed: {:#?}",
        outcome
    );

    let view = v.get_proposal(p2).await?;
    assert!(
        !view["snapshot_and_state"].is_null(),
        "Snapshot should be persisted"
    );
    let for_count = view["votes"][0]["total_votes"].as_u64().unwrap();
    assert_eq!(for_count, 1, "One For vote should be recorded");
    let stored: Option<u8> = v
        .sandbox
        .view(v.voting_id(), "get_vote")
        .args_json(json!({ "account_id": user_a.id(), "proposal_id": p2 }))
        .await?
        .json()?;
    assert_eq!(stored, Some(VoteOption::For as u8));
    assert_eq!(get_status(&view)?, ProposalStatus::Sandbox);

    // Pre-fetch user_b's proof while the venear tree still matches the proposal snapshot.
    let (merkle_proof_b, v_account_b): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({ "account_id": user_b.id() }))
        .await?
        .json()?;

    // user_b can't reuse user_a's v_account — pre-chain check rejects.
    let outcome = user_b
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({
            "proposal_id": p2,
            "vote": {
                "vote": VoteOption::For,
                "merkle_proof": merkle_proof_b,
                "v_account": v_account,
            },
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(150))
        .transact()
        .await?;
    assert!(outcome.is_failure(), "v_account mismatch must fail");
    assert!(
        format!("{:?}", outcome).contains("v_account does not match the caller"),
        "Expected v_account-mismatch panic, got: {:#?}",
        outcome
    );

    // user_c joined after the snapshot was taken
    let user_c = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user_c, NearToken::from_near(100))
        .await?;
    let (merkle_proof, v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({ "account_id": user_c.id() }))
        .await?
        .json()?;
    let outcome = user_c
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({
            "proposal_id": p2,
            "vote": {
                "vote": VoteOption::For,
                "merkle_proof": merkle_proof,
                "v_account": v_account,
            },
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(150))
        .transact()
        .await?;
    assert!(outcome.is_failure(), "Stale-snapshot vote must fail");
    assert!(
        format!("{:?}", outcome).contains("Invalid merkle proof"),
        "Expected Invalid merkle proof panic, got: {:#?}",
        outcome
    );

    // Snapshot already set: user_b's chained call should reuse it (no refetch).
    let stored_root = view["snapshot_and_state"]["snapshot"]["root"].clone();
    let outcome = user_b
        .call(v.voting_id(), "take_snapshot_and_vote")
        .args_json(json!({
            "proposal_id": p2,
            "vote": {
                "vote": VoteOption::For,
                "merkle_proof": merkle_proof_b,
                "v_account": v_account_b,
            },
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(150))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "take_snapshot_and_vote with existing snapshot must succeed: {:#?}",
        outcome
    );
    let view = v.get_proposal(p2).await?;
    assert_eq!(
        view["snapshot_and_state"]["snapshot"]["root"], stored_root,
        "Snapshot root must not change — chain should skip get_snapshot"
    );
    let stored: Option<u8> = v
        .sandbox
        .view(v.voting_id(), "get_vote")
        .args_json(json!({ "account_id": user_b.id(), "proposal_id": p2 }))
        .await?
        .json()?;
    assert_eq!(stored, Some(VoteOption::For as u8));

    Ok(())
}
