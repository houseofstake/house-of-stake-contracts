#![allow(dead_code)]

use super::{VOTING_WASM_FILEPATH, VenearTestWorkspace};
use common::voting::{MajorityType, ProposalStatus, QueueState};
use near_sdk::{Gas, NearToken};
use serde_json::json;

pub fn get_status(
    proposal: &serde_json::Value,
) -> Result<ProposalStatus, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(proposal["status"].clone())?)
}

pub async fn attempt_voting_upgrade(
    user: &near_workspaces::Account,
    v: &VenearTestWorkspace,
) -> Result<(), Box<dyn std::error::Error>> {
    let voting_wasm = std::fs::read(VOTING_WASM_FILEPATH)?;

    let outcome = user
        .call(v.voting_id(), "upgrade")
        .args(voting_wasm)
        .gas(Gas::from_tgas(100))
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

pub async fn create_proposal(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
    actions: Option<serde_json::Value>,
) -> Result<u32, Box<dyn std::error::Error>> {
    let outcome = user
        .call(v.voting_id(), "create_proposal")
        .args_json(json!({
            "metadata": {
                "title": "Test Proposal",
                "description": "This is a test proposal",
            },
            "actions": actions,
            "flow": "Classic",
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

pub async fn create_proposal_fst(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
    actions: Option<serde_json::Value>,
) -> Result<u32, Box<dyn std::error::Error>> {
    let outcome = user
        .call(v.voting_id(), "create_proposal")
        .args_json(json!({
            "metadata": {
                "title": "Test Proposal",
                "description": "This is a test proposal",
            },
            "actions": actions,
            "flow": "FastTrack",
        }))
        .deposit(NearToken::from_millinear(300))
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

pub async fn execute_proposal(
    v: &VenearTestWorkspace,
    executor: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = executor
        .call(v.voting_id(), "execute_proposal")
        .args_json(json!({ "proposal_id": proposal_id }))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;

    Ok(outcome)
}

pub async fn approve_proposal(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
    proposal_id: u32,
    majority_type: MajorityType,
) -> Result<(), Box<dyn std::error::Error>> {
    let outcome = user
        .call(v.voting_id(), "approve_proposal")
        .args_json(json!({
            "proposal_id": proposal_id,
            "majority_type": majority_type,
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

pub async fn vote_for_option(
    v: &VenearTestWorkspace,
    user: &near_workspaces::Account,
    proposal_id: u32,
    option: impl near_sdk::serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (merkle_proof, v_account): (serde_json::Value, serde_json::Value) = v
        .sandbox
        .view(v.venear.id(), "get_proof")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;

    let outcome = user
        .call(v.voting_id(), "vote")
        .args_json(json!({
            "proposal_id": proposal_id,
            "vote": option,
            "merkle_proof": merkle_proof,
            "v_account": v_account,
        }))
        .deposit(NearToken::from_millinear(15))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;

    assert!(outcome.is_success(), "Failed to vote: {:#?}", outcome);

    Ok(())
}

pub async fn slash_proposal(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "slash_proposal")
        .args_json(json!({ "proposal_id": proposal_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    Ok(outcome)
}

pub async fn claim_bond(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "claim_bond")
        .args_json(json!({ "proposal_id": proposal_id }))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    Ok(outcome)
}

pub async fn get_queue_state(
    v: &VenearTestWorkspace,
) -> Result<QueueState, Box<dyn std::error::Error>> {
    Ok(v.sandbox
        .view(v.voting_id(), "get_queue_state")
        .await?
        .json()?)
}

/// Call advance_queue as any account. Returns the execution outcome.
pub async fn advance_queue(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "advance_queue")
        .args_json(json!({}))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    Ok(outcome)
}

/// Reviewer-only reject helper. Valid only while the proposal is in Created status.
pub async fn reject_proposal(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "reject_proposal")
        .args_json(json!({ "proposal_id": proposal_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    Ok(outcome)
}

/// Council-only veto helper.
/// Classic: valid during Timelock. FastTrack: valid during Scheduled or Voting.
pub async fn veto_proposal(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "veto_proposal")
        .args_json(json!({ "proposal_id": proposal_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;
    Ok(outcome)
}

/// Council-only noveto helper. Classic-only; releases a proposal early from Timelock.
pub async fn noveto_proposal(
    v: &VenearTestWorkspace,
    caller: &near_workspaces::Account,
    proposal_id: u32,
) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
    let outcome = caller
        .call(v.voting_id(), "noveto_proposal")
        .args_json(json!({ "proposal_id": proposal_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(250))
        .transact()
        .await?;
    Ok(outcome)
}
