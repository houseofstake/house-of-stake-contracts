mod setup;

use crate::setup::{
    VenearTestWorkspace, VenearTestWorkspaceBuilder, VOTING_DURATION_SECONDS, VOTING_WASM_FILEPATH,
};
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

    let num_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_proposals")
        .await?
        .json()?;
    assert_eq!(num_proposals, 0);

    let proposal_id = create_proposal(&v, &user_a).await?;
    let num_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_proposals")
        .await?
        .json()?;
    assert_eq!(num_proposals, 1);
    let num_approved_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_approved_proposals")
        .await?
        .json()?;
    assert_eq!(num_approved_proposals, 0);

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["status"].as_str().unwrap(), "Created");

    assert!(
        approve_proposal(&v, &user_a, proposal_id).await.is_err(),
        "Regular user should not be able to approve the proposal"
    );

    approve_proposal(&v, &v.voting.as_ref().unwrap().reviewer, proposal_id).await?;

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["total_votes"]["total_votes"].as_u64().unwrap(), 0);
    assert_eq!(proposal["status"].as_str().unwrap(), "Voting");
    assert_eq!(
        proposal["reviewer_id"].as_str().unwrap(),
        v.voting.as_ref().unwrap().reviewer.id().as_str()
    );
    let num_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_proposals")
        .await?
        .json()?;
    assert_eq!(num_proposals, 1);
    let num_approved_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_approved_proposals")
        .await?
        .json()?;
    assert_eq!(num_approved_proposals, 1);

    let (user_a_merkle_proof, user_a_v_account) = v.get_proof(user_a.id()).await?;

    let (user_b_merkle_proof, user_b_v_account) = v.get_proof(user_b.id()).await?;

    let user_c = v.create_account_with_lockup().await?;

    let (user_c_merkle_proof, user_c_v_account) = v.get_proof(user_c.id()).await?;

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

    let vote: Option<u8> = v
        .sandbox
        .view(v.voting_id(), "get_vote")
        .args_json(json!({
            "account_id": user_a.id(),
            "proposal_id": proposal_id,
        }))
        .await?
        .json()?;
    assert_eq!(vote, Some(1));

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
async fn test_voting_reject_proposal() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user_a = v.create_account_with_lockup().await?;

    let proposal_id = create_proposal(&v, &user_a).await?;
    let outcome = user_a
        .call(v.voting_id(), "reject_proposal")
        .args_json(json!({
            "proposal_id": proposal_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to reject proposal: {:#?}",
        outcome
    );

    let outcome = v
        .voting
        .as_ref()
        .unwrap()
        .reviewer
        .call(v.voting_id(), "reject_proposal")
        .args_json(json!({
            "proposal_id": proposal_id,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Reviewer should be able to reject proposal: {:#?}",
        outcome
    );

    let num_approved_proposals: u32 = v
        .sandbox
        .view(v.voting_id(), "get_num_approved_proposals")
        .await?
        .json()?;
    assert_eq!(num_approved_proposals, 0);

    let proposal = v.get_proposal(proposal_id).await?;
    assert_eq!(proposal["status"].as_str().unwrap(), "Rejected");

    Ok(())
}

#[tokio::test]
async fn test_voting_governance() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    let voting_owner = v.voting.as_ref().unwrap().owner.clone();

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

    // Change config with the owner
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

    // Guardians

    let original_guardians: Vec<AccountId> =
        serde_json::from_value(original_config["guardians"].clone())?;
    let new_guardian = v.sandbox.dev_create_account().await?;

    let new_guardians: Vec<AccountId> =
        vec!["new_guardian_1.near".parse()?, new_guardian.id().clone()];
    assert_ne!(original_guardians, new_guardians);

    // Attempt set_guardians
    let outcome = user
        .call(v.voting_id(), "set_guardians")
        .args_json(json!({
            "guardians": new_guardians
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set guardians",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let guardians: Vec<AccountId> = serde_json::from_value(config["guardians"].clone())?;
    assert_eq!(guardians, original_guardians);

    let outcome = voting_owner
        .call(v.voting_id(), "set_guardians")
        .args_json(json!({
            "guardians": new_guardians
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set guardians",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let guardians: Vec<AccountId> = serde_json::from_value(config["guardians"].clone())?;
    assert_eq!(guardians, new_guardians);

    // Change owner account ID
    let new_owner_account = v.sandbox.dev_create_account().await?;
    let original_owner_account_id: AccountId =
        serde_json::from_value(original_config["owner_account_id"].clone())?;
    let new_owner_account_id: AccountId = new_owner_account.id().clone();
    assert_ne!(original_owner_account_id, new_owner_account_id);

    // Attempt propose_new_owner_account_id
    let outcome = user
        .call(v.voting_id(), "propose_new_owner_account_id")
        .args_json(json!({
            "new_owner_account_id": new_owner_account_id
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to propose new owner_account_id",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    let outcome = voting_owner
        .call(v.voting_id(), "propose_new_owner_account_id")
        .args_json(json!({
            "new_owner_account_id": new_owner_account_id
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to propose new owner_account_id",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert_eq!(
        proposed_new_owner_account_id.as_ref(),
        Some(&new_owner_account_id)
    );

    // Cancel proposal
    let outcome = voting_owner
        .call(v.voting_id(), "propose_new_owner_account_id")
        .args_json(json!({
            "new_owner_account_id": None::<String>
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "The current owner should be able to cancel the proposal"
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    let outcome = voting_owner
        .call(v.voting_id(), "propose_new_owner_account_id")
        .args_json(json!({
            "new_owner_account_id": new_owner_account_id
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to propose new owner_account_id",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert_eq!(
        proposed_new_owner_account_id.as_ref(),
        Some(&new_owner_account_id)
    );

    // Accept the ownership by different account
    let outcome = user
        .call(v.voting_id(), "accept_ownership")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to accept the ownership",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert_eq!(
        proposed_new_owner_account_id.as_ref(),
        Some(&new_owner_account_id)
    );

    // Accept ownership by the new owner
    let outcome = new_owner_account
        .call(v.voting_id(), "accept_ownership")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "The new owner should be able to accept the ownership",
    );

    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, new_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    // Propose a config with the new owner
    let outcome = new_owner_account
        .call(v.voting_id(), "propose_new_owner_account_id")
        .args_json(json!({
            "new_owner_account_id": original_owner_account_id,
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
    assert_eq!(owner_account_id, new_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(new_config["proposed_new_owner_account_id"].clone())?;
    assert_eq!(
        proposed_new_owner_account_id.as_ref(),
        Some(&original_owner_account_id)
    );

    Ok(())
}

#[tokio::test]
async fn test_voting_pause() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;
    let user_a = v.create_account_with_lockup().await?;
    let user_b = v.create_account_with_lockup().await?;
    let voting_owner = &v.voting.as_ref().unwrap().owner;

    // Attempt to pause the contract
    let outcome = user_a
        .call(v.voting_id(), "pause")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to pause the contract",
    );

    let is_paused: bool = v
        .sandbox
        .view(v.voting_id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(!is_paused, "Contract should not be paused");

    // Pause the contract by the guardian
    let outcome = v
        .voting
        .as_ref()
        .unwrap()
        .guardian
        .call(v.voting_id(), "pause")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Guardian should be able to pause the contract",
    );

    let is_paused: bool = v
        .sandbox
        .view(v.voting_id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(is_paused, "Contract should be paused");

    // Check if guardian can unpause the contract
    let outcome = v
        .voting
        .as_ref()
        .unwrap()
        .guardian
        .call(v.voting_id(), "unpause")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Guardian should not be able to unpause the contract",
    );

    let is_paused: bool = v
        .sandbox
        .view(v.voting_id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(is_paused, "Contract should be paused");

    // Unpause the contract by the owner
    let outcome = voting_owner
        .call(v.voting_id(), "unpause")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to unpause the contract",
    );

    let is_paused: bool = v
        .sandbox
        .view(v.voting_id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(!is_paused, "Contract should not be paused");

    // Prepare for pause testing
    let proposal_id = create_proposal(&v, &user_a).await?;
    approve_proposal(&v, &v.voting.as_ref().unwrap().reviewer, proposal_id).await?;
    let proposal_id_2 = create_proposal(&v, &user_a).await?;
    assert_ne!(proposal_id, proposal_id_2);

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

    // Pause the contract by the owner
    let outcome = voting_owner
        .call(v.voting_id(), "pause")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to pause the contract",
    );
    let is_paused: bool = v
        .sandbox
        .view(v.voting_id(), "is_paused")
        .await?
        .json()
        .unwrap();

    assert!(is_paused, "Contract should be paused");

    // Attempt to change vote while paused
    let outcome = user_a
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 0,
            "merkle_proof": user_a_merkle_proof,
            "v_account": user_a_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "user_a: Voted while paused: {:#?}",
        outcome
    );

    // Attempt to vote by user_b while paused
    let outcome = user_b
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": 1,
            "merkle_proof": user_b_merkle_proof,
            "v_account": user_b_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "user_b: Voted while paused: {:#?}",
        outcome
    );

    // Attempt to create a proposal while paused
    let outcome = user_b
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
        outcome.is_failure(),
        "user_b: Created proposal while paused: {:#?}",
        outcome
    );

    // Attempt to approve a proposal while paused
    assert!(
        approve_proposal(&v, &v.voting.as_ref().unwrap().reviewer, proposal_id_2)
            .await
            .is_err(),
        "Reviewer should not be able to approve proposal while paused"
    );

    // Attempt to reject a proposal while paused
    let outcome = voting_owner
        .call(v.voting_id(), "reject_proposal")
        .args_json(json!({
            "proposal_id": proposal_id_2,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Reviewer should not be able to reject proposal while paused: {:#?}",
        outcome
    );

    Ok(())
}

#[tokio::test]
async fn test_voting_quorum() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .with_voting()
        .build()
        .await?;

    // Create multiple users with different lockup amounts
    let user_a = v.create_account_with_lockup().await?;
    let user_b = v.create_account_with_lockup().await?;
    let user_c = v.create_account_with_lockup().await?;
    let user_d = v.create_account_with_lockup().await?;

    // Lock NEAR for each user with different amounts
    // We need this to be able to test the quorum unlike the votes
    // above that vote with 1 yoctoNEAR
    v.transfer_and_lock(&user_a, NearToken::from_near(1000))
        .await?;
    v.transfer_and_lock(&user_b, NearToken::from_near(500))
        .await?;
    v.transfer_and_lock(&user_c, NearToken::from_near(200))
        .await?;
    v.transfer_and_lock(&user_d, NearToken::from_near(100))
        .await?;

    // Fast forward to ensure lockups are active
    let current_timestamp = v.sandbox.view_block().await?.timestamp();
    v.fast_forward(current_timestamp + 5_000_000_000, 10, 20)
        .await?;

    // Test Case 1: Create proposal with default quorum (30%)
    let proposal_id_default = create_proposal(&v, &user_a).await?;

    // Test Case 2: Create proposal with custom quorum (50%)
    let outcome = user_b
        .call(v.voting_id(), "create_proposal")
        .args_json(json!({
            "metadata": {
                "title": "Test Proposal with 50% Quorum",
                "description": "This proposal requires 50% quorum",
                "voting_options": ["Yes", "No"],
                "quorum_percentage": 50,
            },
        }))
        .deposit(NearToken::from_millinear(200))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to create proposal with custom quorum"
    );
    let proposal_id_custom: u32 = outcome.json().unwrap();

    // Test Case 3: Create proposal with invalid quorum (>100%)
    let outcome = user_c
        .call(v.voting_id(), "create_proposal")
        .args_json(json!({
            "metadata": {
                "title": "Test Proposal with Invalid Quorum",
                "description": "This should fail",
                "voting_options": ["Yes", "No"],
                "quorum_percentage": 101,
            },
        }))
        .deposit(NearToken::from_millinear(200))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Should fail with invalid quorum percentage"
    );

    // Approve both proposals as the reviewer
    approve_proposal(
        &v,
        &v.voting.as_ref().unwrap().reviewer,
        proposal_id_default,
    )
    .await?;
    approve_proposal(&v, &v.voting.as_ref().unwrap().reviewer, proposal_id_custom).await?;

    // Check proposal details
    let proposal_default = v.get_proposal(proposal_id_default).await?;

    // Confirm the default quorum percentage is 30%
    assert_eq!(proposal_default["quorum_percentage"].as_u64().unwrap(), 30);
    assert_eq!(proposal_default["status"].as_str().unwrap(), "Voting");

    // Confirm the custom quorum percentage is 50%
    let proposal_custom = v.get_proposal(proposal_id_custom).await?;
    assert_eq!(proposal_custom["quorum_percentage"].as_u64().unwrap(), 50);

    // Get merkle proofs for voting
    let (user_a_merkle_proof, user_a_v_account) = v.get_proof(user_a.id()).await?;

    let (user_b_merkle_proof, user_b_v_account) = v.get_proof(user_b.id()).await?;

    // Get total veNEAR supply from snapshot (after approval when snapshot exists)
    let proposal_default_data = v.get_proposal(proposal_id_default).await?;
    let total_venear_str = proposal_default_data["snapshot_and_state"]["total_venear"]
        .as_str()
        .unwrap();
    let total_venear: u128 = total_venear_str.parse().unwrap();

    // Test scenario 1: Vote on default quorum proposal with only 20% participation (below 30% quorum)
    let outcome = user_c
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id_default,
            "vote": 0,
            "merkle_proof": v.sandbox.view(v.venear.id(), "get_proof")
                .args_json(json!({ "account_id": user_c.id() }))
                .await?
                .json::<(serde_json::Value, serde_json::Value)>()?
                .0,
            "v_account": v.sandbox.view(v.venear.id(), "get_proof")
                .args_json(json!({ "account_id": user_c.id() }))
                .await?
                .json::<(serde_json::Value, serde_json::Value)>()?
                .1,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(outcome.is_success(), "Failed to vote");

    // Test scenario 2: Vote on custom quorum proposal with sufficient participation
    // Vote with user_a and user_b (1500 NEAR out of ~1800 total = ~83% > 50% quorum)
    let outcome = user_a
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id_custom,
            "vote": 0,
            "merkle_proof": user_a_merkle_proof,
            "v_account": user_a_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "user_a failed to vote: {:#?}",
        outcome.failures()
    );

    let outcome = user_b
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id_custom,
            "vote": 0,
            "merkle_proof": user_b_merkle_proof,
            "v_account": user_b_v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(outcome.is_success(), "user_b failed to vote");

    // Check voting progress
    let proposal_custom = v.get_proposal(proposal_id_custom).await?;
    let total_voted = proposal_custom["total_votes"]["total_venear"]
        .as_str()
        .unwrap()
        .parse::<u128>()
        .unwrap();
    let quorum_required = total_venear * 50 / 100;
    assert!(
        total_voted >= quorum_required,
        "Should have enough votes for quorum"
    );

    // Fast forward past voting period
    let current_timestamp = v.sandbox.view_block().await?.timestamp();
    v.fast_forward(
        current_timestamp + VOTING_DURATION_SECONDS * 1_000_000_000 + 1_000_000_000,
        10,
        20,
    )
    .await?;

    // Check that proposal failed due to quorum not met
    let proposal_default = v.get_proposal(proposal_id_default).await?;
    assert_eq!(proposal_default["status"].as_str().unwrap(), "QuorumNotMet");

    // Check that proposal passed with quorum met
    let proposal_custom = v.get_proposal(proposal_id_custom).await?;
    assert_eq!(proposal_custom["status"].as_str().unwrap(), "Finished");

    // Test governance: Update default quorum percentage
    let voting_owner = &v.voting.as_ref().unwrap().owner;
    let outcome = voting_owner
        .call(v.voting_id(), "set_default_quorum_percentage")
        .args_json(json!({
            "default_quorum_percentage": 40,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to update default quorum percentage"
    );

    // Verify the config was updated
    let config: serde_json::Value = v.sandbox.view(v.voting_id(), "get_config").await?.json()?;
    assert_eq!(config["default_quorum_percentage"].as_u64().unwrap(), 40);

    // Test that regular user cannot update quorum
    let outcome = user_a
        .call(v.voting_id(), "set_default_quorum_percentage")
        .args_json(json!({
            "default_quorum_percentage": 20,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Regular user should not be able to update quorum"
    );

    Ok(())
}
