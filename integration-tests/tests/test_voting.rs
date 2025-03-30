mod setup;

use crate::setup::{VenearTestWorkspace, VenearTestWorkspaceBuilder, VOTING_WASM_FILEPATH};
use near_sdk::json_types::U64;
use near_sdk::{Gas, NearToken};
use near_workspaces::AccountId;
use serde_json::json;

async fn attempt_voting_upgrade(
    user: &near_workspaces::Account,
    v: &VenearTestWorkspace,
) -> Result<(), Box<dyn std::error::Error>> {
    let voting_wasm = std::fs::read(VOTING_WASM_FILEPATH)?;

    let outcome = user
        .call(v.voting_id(), "upgrade")
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
        .call(v.voting_id(), "create_proposal")
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
        .call(v.voting_id(), "approve_proposal")
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
            "account_id": user_c.id(),
        }))
        .await?
        .json()?;

    let outcome = user_a
        .call(v.voting_id(), "vote")
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
        .call(v.voting_id(), "vote")
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
        .call(v.voting_id(), "vote")
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
        .call(v.voting_id(), "vote")
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
        .call(v.voting_id(), "vote")
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
        .call(v.voting_id(), "vote")
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

#[tokio::test]
async fn test_voting_governance() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    let voting_owner = &v.voting.as_ref().unwrap().owner;

    let original_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;

    let original_venear_account_id: AccountId =
        serde_json::from_value(original_config["venear_account_id"].clone())?;
    let new_venear_account_id: AccountId = "new_venear_account_id".parse()?;
    assert_ne!(original_venear_account_id, new_venear_account_id);

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_venear_account_id")
        .args_json(json!({
            "venear_account_id": new_venear_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Vhange config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_venear_account_id")
        .args_json(json!({
            "venear_account_id": new_venear_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let venear_account_id: AccountId =
        serde_json::from_value(new_config["venear_account_id"].clone())?;
    assert_eq!(venear_account_id, new_venear_account_id);

    // Reviewers
    let original_reviewer_ids: Vec<AccountId> =
        serde_json::from_value(original_config["reviewer_ids"].clone())?;
    let new_reviewer_ids: Vec<AccountId> =
        vec!["new_reviewer_1".parse()?, "new_reviewer_2".parse()?];
    assert_ne!(original_reviewer_ids, new_reviewer_ids);

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_reviewer_ids")
        .args_json(json!({
            "reviewer_ids": new_reviewer_ids,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Change config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_reviewer_ids")
        .args_json(json!({
            "reviewer_ids": new_reviewer_ids,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let reviewer_ids: Vec<AccountId> = serde_json::from_value(new_config["reviewer_ids"].clone())?;
    assert_eq!(reviewer_ids, new_reviewer_ids);

    // Voting duration
    let original_voting_duration_ns: U64 =
        serde_json::from_value(original_config["voting_duration_ns"].clone())?;
    let new_voting_duration_sec: u32 = 1000;
    let new_voting_duration_ns: U64 = (new_voting_duration_sec as u64 * 10u64.pow(9)).into();
    assert_ne!(original_voting_duration_ns, new_voting_duration_ns);

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_voting_duration")
        .args_json(json!({
            "voting_duration_sec": new_voting_duration_sec,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Change config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_voting_duration")
        .args_json(json!({
            "voting_duration_sec": new_voting_duration_sec,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let voting_duration_ns: U64 = serde_json::from_value(new_config["voting_duration_ns"].clone())?;
    assert_eq!(voting_duration_ns, new_voting_duration_ns);

    // Base proposal fee
    let original_base_proposal_fee: NearToken =
        serde_json::from_value(original_config["base_proposal_fee"].clone())?;
    let new_base_proposal_fee: NearToken = NearToken::from_near(2);
    assert_ne!(original_base_proposal_fee, new_base_proposal_fee);

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_base_proposal_fee")
        .args_json(json!({
            "base_proposal_fee": new_base_proposal_fee,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Change config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_base_proposal_fee")
        .args_json(json!({
            "base_proposal_fee": new_base_proposal_fee,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let base_proposal_fee: NearToken =
        serde_json::from_value(new_config["base_proposal_fee"].clone())?;
    assert_eq!(base_proposal_fee, new_base_proposal_fee);

    // Max number of voting options
    let original_max_number_of_voting_options: u8 =
        serde_json::from_value(original_config["max_number_of_voting_options"].clone())?;
    let new_max_number_of_voting_options: u8 = 10;
    assert_ne!(
        original_max_number_of_voting_options,
        new_max_number_of_voting_options
    );

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_max_number_of_voting_options")
        .args_json(json!({
            "max_number_of_voting_options": new_max_number_of_voting_options,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Change config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_max_number_of_voting_options")
        .args_json(json!({
            "max_number_of_voting_options": new_max_number_of_voting_options,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let max_number_of_voting_options: u8 =
        serde_json::from_value(new_config["max_number_of_voting_options"].clone())?;
    assert_eq!(
        max_number_of_voting_options,
        new_max_number_of_voting_options
    );

    // Note, vote storage fee cannot be changed without contract upgrade

    // Change owner account ID
    let original_owner_account_id: AccountId =
        serde_json::from_value(original_config["owner_account_id"].clone())?;
    let new_owner_account = v.sandbox.dev_create_account().await?;
    let new_owner_account_id = new_owner_account.id();
    assert_ne!(&original_owner_account_id, new_owner_account_id);

    // Attempt to change config with a regular user
    let outcome = user
        .call(v.voting_id(), "set_owner_account_id")
        .args_json(json!({
            "owner_account_id": new_owner_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to change config: {:#?}",
        outcome
    );

    // Change config with the owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_owner_account_id")
        .args_json(json!({
            "owner_account_id": new_owner_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId =
        serde_json::from_value(new_config["owner_account_id"].clone())?;
    assert_eq!(&owner_account_id, new_owner_account_id);

    // Attempt to change config with the old owner
    let outcome = voting_owner
        .call(v.voting_id(), "set_owner_account_id")
        .args_json(json!({
            "owner_account_id": original_owner_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Old owner should not be able to change config: {:#?}",
        outcome
    );

    // Attempt to change config with the new owner
    let outcome = new_owner_account
        .call(v.voting_id(), "set_owner_account_id")
        .args_json(json!({
            "owner_account_id": original_owner_account_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "New owner should be able to change config: {:#?}",
        outcome
    );

    let new_config: serde_json::Value =
        v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId =
        serde_json::from_value(new_config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);

    Ok(())
}
