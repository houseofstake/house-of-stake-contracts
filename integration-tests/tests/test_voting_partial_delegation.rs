mod setup;

use crate::setup::venear_helpers::*;
use crate::setup::voting_helpers::*;
use crate::setup::{VenearTestWorkspaceBuilder, assert_almost_eq};
use common::voting::{MajorityType, VoteOption};
use near_sdk::NearToken;

fn vote_venear(proposal: &serde_json::Value, vote_idx: usize) -> NearToken {
    let s = proposal["votes"][vote_idx]["total_venear"]
        .as_str()
        .unwrap_or("0");
    NearToken::from_yoctonear(s.parse().unwrap_or(0))
}

#[tokio::test]
async fn test_partial_delegation_voting() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let alice = v.create_account_with_lockup().await?;
    let bob = v.create_account_with_lockup().await?;
    let carol = v.create_account_with_lockup().await?;

    v.transfer_and_lock(&alice, NearToken::from_near(200))
        .await?;
    v.transfer_and_lock(&bob, NearToken::from_near(50)).await?;
    v.transfer_and_lock(&carol, NearToken::from_near(30))
        .await?;

    set_delegations_sorted(
        &v,
        &alice,
        vec![(bob.id().clone(), 3000), (carol.id().clone(), 2000)],
    )
    .await?;

    let proposal_id = create_proposal(&v, &alice, None).await?;
    approve_proposal(
        &v,
        &v.voting.as_ref().unwrap().reviewer,
        proposal_id,
        MajorityType::Simple,
    )
    .await?;

    // ~0.107% worst-case veNEAR growth observed on the 280 NEAR total over the test
    // runtime; 500 millinear leaves ~60% headroom for sandbox-time variance.
    let delta = NearToken::from_millinear(500);

    // Alice votes For: 200×5000/10000 = 100 (only kept portion)
    vote_for_option(&v, &alice, proposal_id, VoteOption::For).await?;
    let proposal = v.get_proposal(proposal_id).await?;
    assert_almost_eq(vote_venear(&proposal, 0), NearToken::from_near(100), delta);
    assert_almost_eq(vote_venear(&proposal, 1), NearToken::from_near(0), delta);

    // Bob votes For: 50 + 200×3000/10000 = 110; For total = 210
    vote_for_option(&v, &bob, proposal_id, VoteOption::For).await?;
    let proposal = v.get_proposal(proposal_id).await?;
    assert_almost_eq(vote_venear(&proposal, 0), NearToken::from_near(210), delta);
    assert_almost_eq(vote_venear(&proposal, 1), NearToken::from_near(0), delta);

    // Carol votes Against: 30 + 200×2000/10000 = 70
    vote_for_option(&v, &carol, proposal_id, VoteOption::Against).await?;
    let proposal = v.get_proposal(proposal_id).await?;
    let for_venear = vote_venear(&proposal, 0);
    let against_venear = vote_venear(&proposal, 1);
    assert_almost_eq(for_venear, NearToken::from_near(210), delta);
    assert_almost_eq(against_venear, NearToken::from_near(70), delta);

    // Alice 200 (split 100+60+40) + Bob 50 + Carol 30 = 280, no double-counting
    let total = for_venear.as_yoctonear() + against_venear.as_yoctonear();
    assert_almost_eq(
        NearToken::from_yoctonear(total),
        NearToken::from_near(280),
        delta,
    );

    Ok(())
}
