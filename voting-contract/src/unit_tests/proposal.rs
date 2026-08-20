use crate::metadata::ProposalMetadata;
use crate::proposal::*;
use crate::*;
use near_sdk::json_types::U64;

#[cfg(test)]
mod lifecycle_tests {
    //! Lifecycle boundary tests driven through the public `Contract` API; reading via `get_proposal` triggers the implicit `update()`.

    use super::*;
    use crate::unit_tests::test_utils::*;

    #[test]
    fn approve_classic_proposal_moves_to_voting_status() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(100))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Voting
        );
    }

    #[test]
    fn approve_fasttrack_proposal_moves_to_sandbox_status() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(100))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Sandbox
        );
    }

    #[test]
    fn classic_voting_before_end_is_still_voting() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end - 1),
            ProposalStatus::Voting
        );
    }

    #[test]
    fn classic_voting_at_end_with_actions_enters_timelock() {
        // Non-empty action list at creation makes the post-voting branch hit the Timelock arm.
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
            ProposalMetadata {
                title: Some("t".to_string()),
                description: None,
                link: None,
            },
            Some(vec![ProposalAction::Transfer {
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
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Timelock
        );
    }

    #[test]
    fn classic_voting_with_zero_timelock_skips_to_succeeded() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_timelock_duration(0);
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn classic_voting_after_end_failing_quorum_is_defeated() {
        // No votes cast -> total_votes = 0 -> below the 35% quorum.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn classic_timelock_then_succeeded_signaling_only() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        let timelock_end = voting_end + default_config().timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, timelock_end - 1),
            ProposalStatus::Timelock
        );
        assert_eq!(
            status_at(&contract, id, timelock_end),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn created_proposal_expires_exactly_at_deadline() {
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_classic_proposal_expiration(3600);
        let id = create_proposal(&mut contract, ProposalFlow::Classic);

        let expiration_ns = TEST_NOW_NS + contract.get_config().classic_proposal_expiration_ns.0;
        assert_eq!(
            status_at(&contract, id, expiration_ns - 1),
            ProposalStatus::Created
        );
        assert_eq!(
            status_at(&contract, id, expiration_ns),
            ProposalStatus::Expired
        );
    }

    #[test]
    fn created_proposal_with_disabled_expiration_stays_created() {
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_classic_proposal_expiration(0);
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        let very_far = TEST_NOW_NS + 10 * 365 * 24 * 3600 * 1_000_000_000;
        assert_eq!(status_at(&contract, id, very_far), ProposalStatus::Created);
    }

    #[test]
    fn sandbox_at_duration_end_becomes_defeated() {
        // Sandbox threshold never cleared, so the proposal is Defeated once the window elapses.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(50))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        // 50 < 100 (10% of 1000) so the threshold isn't met.
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let sandbox_end = TEST_NOW_NS + default_config().sandbox_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, sandbox_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn sandbox_threshold_met_promotes_to_scheduled() {
        // For = 300 NEAR exactly hits the 30% sandbox threshold of the 1 000 NEAR supply.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(300))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Scheduled
        );
    }

    #[test]
    fn sandbox_threshold_one_yocto_short_stays_in_sandbox() {
        let threshold = NearToken::from_near(300);
        let just_below = NearToken::from_yoctonear(threshold.as_yoctonear() - 1);
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), just_below)],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Sandbox
        );
    }

    #[test]
    fn voting_outcome_quorum_exact_and_approval_exact_succeeds() {
        // 175 For + 175 Against = 350 (quorum 35% exact), approval 50% exact -> Succeeded.
        let fixture = snapshot_with_voters(
            &[
                VoterSpec::new(for_voter(), NearToken::from_near(175)),
                VoterSpec::new(against_voter(), NearToken::from_near(175)),
            ],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        cast_vote(
            &mut contract,
            &fixture,
            against_voter(),
            id,
            VoteOption::Against,
        );

        // After timelock, the signaling-only proposal lands as Succeeded.
        let cfg = default_config();
        let after_timelock =
            TEST_NOW_NS + cfg.classic_voting_duration_ns.0 + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, after_timelock),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn voting_outcome_approval_one_yocto_below_is_defeated() {
        // For = 200 NEAR - 1 yocto vs Against = 200: approval just below 50% -> lose.
        let for_v = NearToken::from_yoctonear(NearToken::from_near(200).as_yoctonear() - 1);
        let fixture = snapshot_with_voters(
            &[
                VoterSpec::new(for_voter(), for_v),
                VoterSpec::new(against_voter(), NearToken::from_near(200)),
            ],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        cast_vote(
            &mut contract,
            &fixture,
            against_voter(),
            id,
            VoteOption::Against,
        );

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn voting_outcome_pure_abstain_is_defeated_even_with_quorum() {
        // 500 Abstain meets quorum but For+Against is empty (zero denominator) -> Defeated.
        let fixture = snapshot_with_voters(
            &[
                VoterSpec::new(for_voter(), NearToken::from_near(0)),
                VoterSpec::new(abstain_voter(), NearToken::from_near(500)),
            ],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(
            &mut contract,
            &fixture,
            abstain_voter(),
            id,
            VoteOption::Abstain,
        );

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn voting_outcome_quorum_floor_dominates_bps_when_higher() {
        // Floor 400 overrides 35% quorum (350); For+Against = 350 < floor -> Defeated.
        let fixture = snapshot_with_voters(
            &[
                VoterSpec::new(for_voter(), NearToken::from_near(175)),
                VoterSpec::new(against_voter(), NearToken::from_near(175)),
            ],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_quorum_floor(NearToken::from_near(400));
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        cast_vote(
            &mut contract,
            &fixture,
            against_voter(),
            id,
            VoteOption::Against,
        );

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn voting_outcome_for_only_succeeds() {
        // For = 400, Against = 0: quorum met, approval 100% -> pure-For winning path.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let cfg = default_config();
        let after_timelock =
            TEST_NOW_NS + cfg.classic_voting_duration_ns.0 + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, after_timelock),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn voting_outcome_against_only_is_defeated() {
        // For = 0, Against = 400: quorum met, approval 0% — distinct from abstain-only (denominator = 0).
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(against_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(
            &mut contract,
            &fixture,
            against_voter(),
            id,
            VoteOption::Against,
        );

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn voting_outcome_quorum_one_yocto_below_is_defeated() {
        // Quorum = 35% of 1000 = 350 NEAR. Cast 350 NEAR - 1 yocto total. Below.
        let for_amount = NearToken::from_yoctonear(NearToken::from_near(350).as_yoctonear() - 1);
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), for_amount)],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let voting_end = TEST_NOW_NS + default_config().classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn fasttrack_voting_after_end_failing_quorum_is_defeated() {
        // For = 300 clears sandbox -> Scheduled, but 300 < 350 quorum -> Defeated at voting_end.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(300))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Scheduled
        );

        let voting_start = next_voting_start_ns(TEST_NOW_NS);
        let voting_end = voting_start + default_config().fast_track_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Defeated
        );
    }

    #[test]
    fn fasttrack_voting_with_actions_at_end_is_executable() {
        // FastTrack with an action: For=400 clears sandbox and majority -> Executable at voting_end.
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
            ProposalMetadata {
                title: Some("ft-actions".to_string()),
                description: None,
                link: None,
            },
            Some(vec![ProposalAction::Transfer {
                receiver_id: acc("dest.test.near"),
                amount: NearToken::from_near(1),
            }]),
            ProposalFlow::FastTrack,
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

        let voting_start = next_voting_start_ns(TEST_NOW_NS);
        let voting_end = voting_start + default_config().fast_track_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Executable
        );
    }

    #[test]
    fn fasttrack_created_proposal_expires_exactly_at_deadline() {
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_fast_track_proposal_expiration(3600);
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);

        let expiration_ns = TEST_NOW_NS + contract.get_config().fast_track_proposal_expiration_ns.0;
        assert_eq!(
            status_at(&contract, id, expiration_ns - 1),
            ProposalStatus::Created
        );
        assert_eq!(
            status_at(&contract, id, expiration_ns),
            ProposalStatus::Expired
        );
    }

    #[test]
    fn voting_outcome_quorum_floor_satisfied_at_boundary_succeeds() {
        // Same floor as above (400 NEAR). For+Against = 400 NEAR exactly.
        let fixture = snapshot_with_voters(
            &[
                VoterSpec::new(for_voter(), NearToken::from_near(200)),
                VoterSpec::new(against_voter(), NearToken::from_near(200)),
            ],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        set_ctx(owner(), 1, TEST_NOW_NS);
        contract.set_quorum_floor(NearToken::from_near(400));
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        cast_vote(
            &mut contract,
            &fixture,
            against_voter(),
            id,
            VoteOption::Against,
        );

        let cfg = default_config();
        let after_timelock =
            TEST_NOW_NS + cfg.classic_voting_duration_ns.0 + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, after_timelock),
            ProposalStatus::Succeeded
        );
    }

    // active_end_time_ns — direct unit tests on the four flow/status branches that drive queue backdating.

    fn read_raw(contract: &Contract, id: ProposalId) -> Proposal {
        contract.proposals.get(id).cloned().unwrap().into()
    }

    #[test]
    fn active_end_time_classic_succeeded_includes_timelock() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let cfg = default_config();
        let raw = read_raw(&contract, id);
        assert_eq!(
            raw.active_end_time_ns(),
            TEST_NOW_NS + cfg.classic_voting_duration_ns.0 + cfg.timelock_duration_ns.0
        );
    }

    #[test]
    fn active_end_time_classic_defeated_is_voting_end() {
        // No votes -> Defeated -> active_end is voting_end, not voting_end + timelock.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, id, Some(&fixture));

        let raw = read_raw(&contract, id);
        assert_eq!(
            raw.active_end_time_ns(),
            TEST_NOW_NS + default_config().classic_voting_duration_ns.0
        );
    }

    #[test]
    fn active_end_time_fasttrack_with_voting_start_is_voting_end() {
        // For = 400 flips sandbox -> Scheduled, sets voting_start_time_ns.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        let raw = read_raw(&contract, id);
        let voting_start = raw.voting_start_time_ns.unwrap().0;
        assert_eq!(
            raw.active_end_time_ns(),
            voting_start + default_config().fast_track_voting_duration_ns.0
        );
    }

    #[test]
    fn active_end_time_fasttrack_sandbox_only_is_sandbox_end() {
        // No vote -> still Sandbox -> voting_start_time_ns None, active_end = sandbox_start + duration.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(50))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        approve_proposal(&mut contract, id, Some(&fixture));

        let raw = read_raw(&contract, id);
        assert!(raw.voting_start_time_ns.is_none());
        assert_eq!(
            raw.active_end_time_ns(),
            TEST_NOW_NS + default_config().sandbox_duration_ns.0
        );
    }

    #[test]
    fn sandbox_threshold_met_returns_false_without_snapshot() {
        // No snapshot before approval: sandbox_threshold_met must short-circuit on the None branch.
        let mut contract = fresh_contract();
        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        let raw = read_raw(&contract, id);
        assert!(raw.snapshot_and_state.is_none());
        assert_eq!(raw.sandbox_threshold_met(), false);
    }
}

#[cfg(test)]
mod create_proposal_tests {
    //! Boundary/validation tests for `Contract::create_proposal`: deposit arithmetic and action-list validation.

    use super::*;
    use crate::unit_tests::test_utils::*;

    #[test]
    fn create_proposal_classic_assigns_sequential_ids() {
        let mut contract = fresh_contract();

        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        let id_a = contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        let id_b = contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        let id_c = contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);

        assert_eq!(id_a, 0);
        assert_eq!(id_b, 1);
        assert_eq!(id_c, 2);
        assert_eq!(contract.get_num_proposals(), 3);
    }

    #[test]
    #[should_panic(expected = "Actions list cannot be empty")]
    fn create_proposal_rejects_empty_actions_list() {
        let mut contract = fresh_contract();
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        contract.create_proposal(proposal_metadata("t"), Some(vec![]), ProposalFlow::Classic);
    }

    #[test]
    #[should_panic(expected = "Requires deposit of")]
    fn create_proposal_classic_rejects_insufficient_deposit() {
        let mut contract = fresh_contract();
        // Far below the storage fee + base proposal fee.
        set_ctx(proposer(), 1, TEST_NOW_NS);
        contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);
    }

    #[test]
    #[should_panic(expected = "Requires deposit of")]
    fn create_proposal_fasttrack_rejects_deposit_missing_bond() {
        let mut contract = fresh_contract();
        let cfg = default_config();
        // Cover base proposal fee but NOT the bond — falls just short.
        let deposit = cfg.base_proposal_fee.as_yoctonear() + NearToken::from_near(1).as_yoctonear();
        set_ctx(proposer(), deposit, TEST_NOW_NS);
        contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::FastTrack);
    }

    #[test]
    fn create_proposal_classic_with_expiration_records_absolute_deadline() {
        // default_config already enables a non-zero classic expiration; no setter call needed.
        let mut contract = fresh_contract();
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        let id = contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);
        let p: Proposal = contract.proposals.get(id).cloned().unwrap().into();
        assert_eq!(
            p.expiration_ns,
            U64(TEST_NOW_NS + contract.get_config().classic_proposal_expiration_ns.0)
        );
    }

    #[test]
    fn create_proposal_fasttrack_with_expiration_records_absolute_deadline() {
        // default_config already enables a non-zero FastTrack expiration; no setter call needed.
        let mut contract = fresh_contract();
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        let id = contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::FastTrack);
        let p: Proposal = contract.proposals.get(id).cloned().unwrap().into();
        assert_eq!(
            p.expiration_ns,
            U64(TEST_NOW_NS + contract.get_config().fast_track_proposal_expiration_ns.0)
        );
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn create_proposal_when_paused_panics() {
        let mut contract = fresh_contract();
        contract.paused = true;
        set_ctx(proposer(), over_deposit_yocto(), TEST_NOW_NS);
        contract.create_proposal(proposal_metadata("t"), None, ProposalFlow::Classic);
    }

    #[test]
    fn get_proposal_returns_none_for_invalid_id() {
        let contract = fresh_contract();
        assert!(contract.get_proposal(0).is_none());
        assert!(contract.get_proposal(999).is_none());
    }

    #[test]
    fn get_proposals_with_limit_none_returns_all() {
        let mut contract = fresh_contract();
        let _ = create_proposal(&mut contract, ProposalFlow::Classic);
        let _ = create_proposal(&mut contract, ProposalFlow::Classic);
        let _ = create_proposal(&mut contract, ProposalFlow::FastTrack);

        let all = contract.get_proposals(0, None);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].proposal.id, 0);
        assert_eq!(all[1].proposal.id, 1);
        assert_eq!(all[2].proposal.id, 2);
    }

    #[test]
    fn get_proposals_pagination_respects_limit_and_offset() {
        let mut contract = fresh_contract();
        for _ in 0..5 {
            let _ = create_proposal(&mut contract, ProposalFlow::Classic);
        }

        let page = contract.get_proposals(1, Some(2));
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].proposal.id, 1);
        assert_eq!(page[1].proposal.id, 2);
    }

    #[test]
    fn get_proposals_handles_out_of_bounds_from() {
        let mut contract = fresh_contract();
        let _ = create_proposal(&mut contract, ProposalFlow::Classic);
        assert!(contract.get_proposals(10, None).is_empty());
        assert!(contract.get_proposals(10, Some(5)).is_empty());
    }

    #[test]
    fn get_proposals_limit_zero_returns_empty() {
        let mut contract = fresh_contract();
        let _ = create_proposal(&mut contract, ProposalFlow::Classic);
        assert!(contract.get_proposals(0, Some(0)).is_empty());
    }
}

#[cfg(test)]
mod long_flow_tests {
    //! End-to-end tests walking a proposal through every lifecycle stage in sequence via the public `Contract` API.
    use super::*;
    use crate::unit_tests::test_utils::*;

    #[test]
    fn classic_signaling_full_flow_created_to_succeeded() {
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();

        // 1. Create proposal -> Created
        let id = create_proposal(&mut contract, ProposalFlow::Classic);
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Created
        );

        // 2. Reviewer approves -> Voting (with snapshot wired by helper)
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
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Voting
        );

        // 3. Cast a passing For vote.
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);

        // 4. Past voting_end but before timelock_end -> Timelock.
        let cfg = default_config();
        let voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Timelock
        );

        // 5. Past timelock_end with no actions -> Succeeded.
        let timelock_end = voting_end + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, timelock_end),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn classic_full_flow_with_actions_ends_executable() {
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
            ProposalMetadata {
                title: Some("with-actions".to_string()),
                description: None,
                link: None,
            },
            Some(vec![ProposalAction::Transfer {
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

        let cfg = default_config();
        let voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
        let timelock_end = voting_end + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, timelock_end),
            ProposalStatus::Executable
        );
    }

    #[test]
    fn fasttrack_signaling_full_flow_created_to_succeeded() {
        // Voter holds 400 NEAR (40%) — clears both the sandbox and 50% approval thresholds alone.
        let fixture = snapshot_with_voters(
            &[VoterSpec::new(for_voter(), NearToken::from_near(400))],
            NearToken::from_near(1_000),
        );
        let mut contract = fresh_contract();

        let id = create_proposal(&mut contract, ProposalFlow::FastTrack);
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Created
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
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Sandbox
        );

        // Cast the For vote that meets the sandbox threshold -> Scheduled.
        cast_vote(&mut contract, &fixture, for_voter(), id, VoteOption::For);
        assert_eq!(
            status_at(&contract, id, TEST_NOW_NS),
            ProposalStatus::Scheduled
        );

        // Scheduled start is the next Monday CET; reading at that boundary -> Voting.
        let voting_start = next_voting_start_ns(TEST_NOW_NS);
        assert_eq!(
            status_at(&contract, id, voting_start),
            ProposalStatus::Voting
        );

        // Past voting_end -> Succeeded; the For vote is preserved through Sandbox -> Voting.
        let voting_end = voting_start + default_config().fast_track_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, id, voting_end),
            ProposalStatus::Succeeded
        );
    }

    #[test]
    fn concurrent_three_classic_proposals_at_default_cap_lifecycle() {
        // Default cap = 3: three independent proposals with different outcomes land in their terminal status without interference.
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
        approve_proposal(&mut contract, a, Some(&fixture));
        let b = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, b, Some(&fixture));
        let c = create_proposal(&mut contract, ProposalFlow::Classic);
        approve_proposal(&mut contract, c, Some(&fixture));

        assert_active_with_status(&contract, a, ProposalStatus::Voting);
        assert_active_with_status(&contract, b, ProposalStatus::Voting);
        assert_active_with_status(&contract, c, ProposalStatus::Voting);

        // A: 400 For + 100 Against -> approval 80% (>= 50% quorum 35% trivially met) -> Succeeded.
        let (proof, v_account) = fixture.proof_for(&for_voter());
        set_ctx(
            for_voter(),
            NearToken::from_millinear(10).as_yoctonear(),
            TEST_NOW_NS,
        );
        contract.vote(a, VoteOption::For, proof, v_account);
        let (proof, v_account) = fixture.proof_for(&against_voter());
        set_ctx(
            against_voter(),
            NearToken::from_millinear(10).as_yoctonear(),
            TEST_NOW_NS,
        );
        contract.vote(a, VoteOption::Against, proof, v_account);

        // B: only Abstain. Meets quorum but denominator = 0 -> Defeated.
        let (proof, v_account) = fixture.proof_for(&for_voter());
        set_ctx(
            for_voter(),
            NearToken::from_millinear(10).as_yoctonear(),
            TEST_NOW_NS,
        );
        contract.vote(b, VoteOption::Abstain, proof, v_account);

        // C: no votes -> below quorum -> Defeated.

        // At voting_end: A enters Timelock (signaling-only), B and C immediately Defeated.
        let cfg = default_config();
        let voting_end = TEST_NOW_NS + cfg.classic_voting_duration_ns.0;
        assert_eq!(
            status_at(&contract, a, voting_end),
            ProposalStatus::Timelock
        );
        assert_eq!(
            status_at(&contract, b, voting_end),
            ProposalStatus::Defeated
        );
        assert_eq!(
            status_at(&contract, c, voting_end),
            ProposalStatus::Defeated
        );

        // After timelock_end, A drops to Succeeded; B and C unchanged.
        let timelock_end = voting_end + cfg.timelock_duration_ns.0;
        assert_eq!(
            status_at(&contract, a, timelock_end),
            ProposalStatus::Succeeded
        );
        assert_eq!(
            status_at(&contract, b, timelock_end),
            ProposalStatus::Defeated
        );
        assert_eq!(
            status_at(&contract, c, timelock_end),
            ProposalStatus::Defeated
        );

        // active_proposals must be empty now.
        assert!(contract.get_queue_state().active_proposals.is_empty());
    }
}
