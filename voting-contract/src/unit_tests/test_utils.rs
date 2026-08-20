#![cfg(test)]
use crate::Contract;
use crate::config::Config;
use crate::metadata::ProposalMetadata;
use crate::proposal::{MajorityType, ProposalFlow, ProposalId, ProposalStatus, is_active_status};
use chrono::{FixedOffset, NaiveDate};
use common::Bps;
pub use common::test_utils::{
    SnapshotFixture, VMContextBuilder, VoterSpec, abstain_voter, acc, against_voter, council,
    current_account, for_voter, guardian, owner, proposer, reviewer, set_ctx, voter,
};
use common::voting::VoteOption;
use near_sdk::json_types::U64;
use near_sdk::{AccountId, NearToken};

// Named-account fixtures are re-exported above from `common::test_utils`.

/// Test default: 2026-06-01 00:00:00 UTC in nanoseconds, truncated to seconds.
pub const TEST_NOW_NS: u64 = 1_780_272_000_000_000_000;

// ---------------------------------------------------------------------------
// Config and contract construction
// ---------------------------------------------------------------------------

/// Mirrors the production defaults applied by `upgrade.rs`. Tests that need
/// different behavior (e.g. disabled expiration, lower sandbox threshold)
/// must call the matching owner-only setter explicitly.
pub fn default_config() -> Config {
    Config {
        venear_account_id: acc("venear.test.near"),
        reviewer_ids: vec![reviewer()],
        council_ids: vec![council()],
        owner_account_id: owner(),
        classic_voting_duration_ns: U64(14 * 24 * 3600 * 1_000_000_000),
        fast_track_voting_duration_ns: U64(5 * 24 * 3600 * 1_000_000_000),
        timelock_duration_ns: U64(14 * 24 * 3600 * 1_000_000_000),
        base_proposal_fee: NearToken::from_near(1),
        bond_amount: NearToken::from_near(100),
        treasury_account_id: acc("treasury.test.near"),
        vote_storage_fee: NearToken::from_millinear(10),
        guardians: vec![guardian()],
        classic_proposal_expiration_ns: U64(7 * 24 * 3600 * 1_000_000_000),
        fast_track_proposal_expiration_ns: U64(2 * 24 * 3600 * 1_000_000_000),
        proposed_new_owner_account_id: None,
        quorum_threshold_bps: Bps::new(3_500),
        quorum_floor: NearToken::from_yoctonear(0),
        approval_threshold_bps: Bps::new(5_000),
        simple_majority_threshold_bps: Bps::new(5_000),
        strong_majority_threshold_bps: Bps::new(6_667),
        sandbox_duration_ns: U64(7 * 24 * 3600 * 1_000_000_000),
        sandbox_threshold_bps: Bps::new(3_000),
        max_active_proposals: 3,
    }
}

/// The single contract constructor: installs a proposer context at `TEST_NOW_NS`
/// and builds a `Contract` from `default_config()`. Callers override the context
/// afterwards (e.g. `set_ctx(owner(), 1, TEST_NOW_NS)` before owner-only setters).
pub fn fresh_contract() -> Contract {
    set_ctx(proposer(), 0, TEST_NOW_NS);
    Contract::new(default_config())
}

// ---------------------------------------------------------------------------
// Snapshot fixture
// ---------------------------------------------------------------------------

/// Builds a snapshot fixture at `TEST_NOW_NS` with `reviewer()` as the building
/// context. The reusable builder lives in `common::test_utils`; this wrapper
/// pins the voting-test conventions so call sites stay unchanged.
pub fn snapshot_with_voters(voters: &[VoterSpec], total_venear: NearToken) -> SnapshotFixture {
    common::test_utils::snapshot_with_voters(voters, total_venear, TEST_NOW_NS, reviewer())
}

/// Expected voting power of a `near_balance` cast against a `snapshot_with_voters`
/// fixture: the snapshot evaluates balances at `TEST_NOW_NS`, one `LOCK_AGE_NS`
/// after the accounts were written, so growth applies. Computed via the
/// contract's own `total_balance` math to keep assertions exact. Growth accrues
/// only on `near_balance`, so a voter's extra veNEAR adds on top flatly.
pub fn voting_power(near_balance: NearToken) -> NearToken {
    let account: common::account::Account = common::test_utils::make_v_account(
        voter(),
        near_balance,
        NearToken::from_yoctonear(0),
        U64(TEST_NOW_NS - common::test_utils::LOCK_AGE_NS),
    )
    .into();
    account.total_balance(U64(TEST_NOW_NS), &common::test_utils::growth_config())
}

// ---------------------------------------------------------------------------
// Proposal lifecycle (composable primitives — chain as needed)
// ---------------------------------------------------------------------------

const PROPOSER_DEPOSIT_NEAR: u8 = 200;

fn proposer_deposit_yocto() -> u128 {
    NearToken::from_near(PROPOSER_DEPOSIT_NEAR.into()).as_yoctonear()
}

/// Creates a proposal from `proposer()` and returns its id. Leaves it in
/// `Created`. Caller resets the context afterwards as needed.
pub fn create_proposal(contract: &mut Contract, flow: ProposalFlow) -> ProposalId {
    let metadata = proposal_metadata("test proposal");
    set_ctx(proposer(), proposer_deposit_yocto(), TEST_NOW_NS);
    contract.create_proposal(metadata, None, flow)
}

/// Approves a `Created` proposal from `reviewer()` with a `Simple` majority.
/// Tests that need `Strong` call `approve_proposal` on the contract directly.
///
/// If `snapshot` is `Some`, also delivers the venear-callback snapshot tuple
/// (`on_get_snapshot`) so the proposal lands fully active. Pass `None` to stop
/// after approval — used for tests that exercise queueing, the
/// "Snapshot has not been taken yet" guard, or the snapshot callback itself.
pub fn approve_proposal(
    contract: &mut Contract,
    id: ProposalId,
    snapshot: Option<&SnapshotFixture>,
) {
    set_ctx(reviewer(), 1, TEST_NOW_NS);
    let _ = contract.approve_proposal(id, MajorityType::Simple);
    if let Some(fixture) = snapshot {
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
    }
}

/// Builds a `ProposalMetadata` with the given title and no description/link.
pub fn proposal_metadata(title: &str) -> ProposalMetadata {
    ProposalMetadata {
        title: Some(title.to_string()),
        description: None,
        link: None,
    }
}

/// Yocto deposit that covers the maximum required across classic + fasttrack
/// `create_proposal` paths (base_proposal_fee + bond + headroom).
pub fn over_deposit_yocto() -> u128 {
    NearToken::from_near(200).as_yoctonear()
}

// ---------------------------------------------------------------------------
// Vote helpers
// ---------------------------------------------------------------------------

/// Standard vote-storage deposit (10 millinear). Matches `default_config()`'s
/// `vote_storage_fee`.
pub fn vote_deposit_yocto() -> u128 {
    NearToken::from_millinear(10).as_yoctonear()
}

/// Sets up the caller context and casts a vote at `TEST_NOW_NS`.
pub fn cast_vote(
    contract: &mut Contract,
    fixture: &SnapshotFixture,
    voter_id: AccountId,
    proposal_id: ProposalId,
    option: VoteOption,
) {
    cast_vote_at(
        contract,
        fixture,
        voter_id,
        proposal_id,
        option,
        TEST_NOW_NS,
    );
}

/// Sets up the caller context and casts a vote at an explicit block timestamp.
pub fn cast_vote_at(
    contract: &mut Contract,
    fixture: &SnapshotFixture,
    voter_id: AccountId,
    proposal_id: ProposalId,
    option: VoteOption,
    at_ns: u64,
) {
    let (proof, v_account) = fixture.proof_for(&voter_id);
    set_ctx(voter_id, vote_deposit_yocto(), at_ns);
    contract.vote(proposal_id, option, proof, v_account);
}

// ---------------------------------------------------------------------------
// Status / queue assertions
// ---------------------------------------------------------------------------

/// Reads the proposal status as it would appear at the given block time.
/// `get_proposal()` calls `update()` internally, so this lets tests probe
/// the time-boundary arms of `update_classic` / `update_fast_track`.
pub fn status_at(contract: &Contract, id: ProposalId, at_ns: u64) -> ProposalStatus {
    set_ctx(voter(), 0, at_ns);
    contract.get_proposal(id).expect("proposal").proposal.status
}

/// Reads every proposal's status as it would appear at the given block time,
/// in proposal-id (creation) order. Uses `get_proposals` so virtual queue
/// promotions are reflected.
pub fn all_statuses_at(contract: &Contract, at_ns: u64) -> Vec<ProposalStatus> {
    set_ctx(voter(), 0, at_ns);
    contract
        .get_proposals(0, None)
        .into_iter()
        .map(|p| p.proposal.status)
        .collect()
}

/// Asserts a proposal is both in `active_proposals` AND has the expected
/// lifecycle-active stored status. Catches drift between the active set and
/// the proposal's stored status.
pub fn assert_active_with_status(contract: &Contract, id: ProposalId, expected: ProposalStatus) {
    assert!(
        is_active_status(expected),
        "expected status {:?} is not a lifecycle-active status — use direct status comparison instead",
        expected
    );
    let state = contract.get_queue_state();
    assert!(
        state.active_proposals.contains(&id),
        "proposal {} missing from active_proposals (got {:?})",
        id,
        state.active_proposals
    );
    let actual = contract
        .get_proposal(id)
        .expect("proposal exists")
        .proposal
        .status;
    assert_eq!(
        actual, expected,
        "proposal {} active set OK but status mismatch",
        id
    );
}

/// Asserts a proposal is in the pending queue at the given position with
/// stored status `Queued` (the only valid status for a pending-queue member).
pub fn assert_queued_at(contract: &Contract, id: ProposalId, position: usize) {
    let state = contract.get_queue_state();
    assert_eq!(
        state.pending_queue.get(position).copied(),
        Some(id),
        "expected proposal {} at queue position {}, got {:?}",
        id,
        position,
        state.pending_queue
    );
    let actual = contract
        .get_proposal(id)
        .expect("proposal exists")
        .proposal
        .status;
    assert_eq!(
        actual,
        ProposalStatus::Queued,
        "proposal {} in pending queue but stored status is {:?}",
        id,
        actual
    );
    assert!(
        !state.active_proposals.contains(&id),
        "proposal {} both queued and active",
        id
    );
}

// ---------------------------------------------------------------------------
// Date math
// ---------------------------------------------------------------------------

/// Returns y-m-d 00:00 CET (fixed UTC+1) as UTC nanoseconds. Used by date-math
/// unit tests for `next_voting_start_ns`.
pub fn date_ns(year: i32, month: u32, day: u32) -> u64 {
    let cet = FixedOffset::east_opt(3600).unwrap();
    u64::try_from(
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(cet)
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap(),
    )
    .unwrap()
}
