mod setup;

use crate::setup::{VenearTestWorkspace, VenearTestWorkspaceBuilder, VOTING_WASM_FILEPATH};
use near_sdk::{Gas, NearToken};
use serde_json::json;

async fn attempt_voting_upgrade(
    user: &near_workspaces::Account,
    v: &VenearTestWorkspace,
) -> Result<(), Box<dyn std::error::Error>> {
    let voting_wasm = std::fs::read(VOTING_WASM_FILEPATH)?;

    let outcome = user
        .call(v.voting.as_ref().unwrap().contract.id(), "upgrade")
        .args(voting_wasm)
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;

    if !outcome.is_success() {
        return Err(format!(
            "Failed to upgrade voting contract: {:#?}",
            outcome.outcomes()
        )
        .into());
    }

    Ok(())
}

#[tokio::test]
async fn test_upgrade_voting() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user_a = v.sandbox.dev_create_account().await?;

    assert!(
        attempt_voting_upgrade(&user_a, &v).await.is_err(),
        "User should not be able to upgrade the contract"
    );

    attempt_voting_upgrade(&v.voting.as_ref().unwrap().owner, &v).await?;

    Ok(())
}

async fn create_proposal(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
) -> Result<u32, Box<dyn std::error::Error>> {
    let outcome = user
        .call(v.voting.as_ref().unwrap().contract.id(), "create_proposal")
        .args_json(json!({
            "metadata": {
                "title": "Test Proposal",
                "description": "This is a test proposal",
                "voting_options": ["Option 1", "Option 2", "Option 3"],
            },
        }))
        .deposit(NearToken::from_millinear(200))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Failed to create proposal {:#?}",
        outcome
    );

    Ok(outcome.json().unwrap())
}

async fn approve_proposal(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let outcome = user
        .call(v.voting.as_ref().unwrap().contract.id(), "approve_proposal")
        .args_json(json!({
            "proposal_id": proposal_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;

    if !outcome.is_success() {
        return Err(format!("Failed to approve proposal: {:#?}", outcome.outcomes()).into());
    }

    Ok(())
}

#[tokio::test]
async fn test_voting() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user_a = v.create_account_with_lockup().await?;
    let user_b = v.create_account_with_lockup().await?;

    let proposal_id = create_proposal(&v, &user_a).await?;

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 0);

    assert!(
        approve_proposal(&v, &user_a, proposal_id).await.is_err(),
        "Regular user should not be able to approve the proposal"
    );

    approve_proposal(&v, &v.voting.as_ref().unwrap().reviewer, proposal_id).await?;

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 0);

    let (user_a_merkle_proof, user_a_v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({
            "account_id": user_a.id(),
        }))
        .await?
        .json()?;

    let (user_b_merkle_proof, user_b_v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({
            "account_id": user_b.id(),
        }))
        .await?
        .json()?;

    let user_c = v.create_account_with_lockup().await?;

    let (user_c_merkle_proof, user_c_v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({
            "account_id": user_b.id(),
        }))
        .await?
        .json()?;

    let outcome = user_a
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 1,
            "merkle_proof": user_a_merkle_proof,
            "v_account": user_a_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "user_a: Failed to vote: {:#?}",
        outcome
    );

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["votes"][0]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["votes"][1]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["votes"][2]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 1);

    // Attempt to vote with an invalid proof
    let outcome = user_b
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 2,
            "merkle_proof": user_a_merkle_proof,
            "v_account": user_b_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "user_b: Voted with invalid proof: {:#?}",
        outcome
    );

    // Attempt to vote from the different account
    let outcome = user_c
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 2,
            "merkle_proof": user_b_merkle_proof,
            "v_account": user_b_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "user_c: Voted for account user_b: {:#?}",
        outcome
    );

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["votes"][0]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["votes"][1]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["votes"][2]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 1);

    // Valid vote from user_b
    let outcome = user_b
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 2,
            "merkle_proof": user_b_merkle_proof,
            "v_account": user_b_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "user_b: Failed to vote: {:#?}",
        outcome
    );

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["votes"][0]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["votes"][1]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["votes"][2]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 2);

    // Attempt to vote from user_c with different root
    let outcome = user_c
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 0,
            "merkle_proof": user_c_merkle_proof,
            "v_account": user_c_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "user_c: Voted after snapshot: {:#?}",
        outcome
    );

    // Changing vote from user_a
    let outcome = user_a
        .call(v.voting.as_ref().unwrap().contract.id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 0,
            "merkle_proof": user_a_merkle_proof,
            "v_account": user_a_v_account,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "user_a: Failed to change vote: {:#?}",
        outcome
    );

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["votes"][0]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["votes"][1]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["votes"][2]["total_votes"].as_u64().unwrap(), 1);
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 2);

    Ok(())
}
