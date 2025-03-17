mod setup;

use crate::setup::{
    outcome_check, VenearTestWorkspace, VenearTestWorkspaceBuilder, UNLOCK_DURATION_SECONDS,
};
use common::near_add;
use near_sdk::{Gas, Timestamp};
use near_workspaces::types::NearToken;
use near_workspaces::Account;
use serde_json::json;

pub async fn transfer_and_lock(
    v: &VenearTestWorkspace,
    user: &Account,
    amount: NearToken,
) -> Result<(), Box<dyn std::error::Error>> {
    let lockup_id = v.get_lockup_account_id(user.id()).await?;

    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(&lockup_id, amount)
        .await?;
    outcome_check(&outcome);

    user.call(&lockup_id, "lock_near")
        .args_json(json!({ "amount": amount }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

#[tokio::test]
async fn test_full_lock_unlock_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_account_id = v.get_lockup_account_id(user.id()).await?;

    // Initial deposit
    let deposit = NearToken::from_near(100);

    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(&lockup_account_id, deposit)
        .await?;
    outcome_check(&outcome);

    let nonce_before = v.get_lockup_update_nonce(&lockup_account_id).await?;

    // Lock 50 NEAR
    let outcome = user
        .call(&lockup_account_id, "lock_near")
        .args_json(json!({ "amount": NearToken::from_near(50) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    outcome_check(&outcome);

    let nonce_after = v.get_lockup_update_nonce(&lockup_account_id).await?;
    assert_eq!(nonce_after, nonce_before + 1, "Nonce should increment");

    let locked = v.get_venear_locked(&lockup_account_id).await?;
    assert_eq!(locked, NearToken::from_near(50));

    // Begin unlock 30 NEAR
    let outcome = user
        .call(&lockup_account_id, "begin_unlock_near")
        .args_json(json!({ "amount": NearToken::from_near(30) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    outcome_check(&outcome);

    let unlock_timestamp = v.get_venear_unlock_timestamp(&lockup_account_id).await?;
    assert!(unlock_timestamp > 0, "venear_unlock_timestamp was not set");

    let pending = v.get_venear_pending(&lockup_account_id).await?;
    assert_eq!(pending, NearToken::from_near(30));
    let locked_after_begin_unlock = v.get_venear_locked(&lockup_account_id).await?;
    assert_eq!(locked_after_begin_unlock, NearToken::from_near(20));

    let mut i = 0_u16;
    while i <= 10 {
        // Fast forward time, number of seconds
        v.sandbox.fast_forward(UNLOCK_DURATION_SECONDS).await?;
        let block = v.sandbox.view_block().await?;
        if block.timestamp() >= unlock_timestamp {
            break;
        } else {
            println!("Unlock timestamp is in the future, waiting...");
        }
        i += 1;
    }
    assert_ne!(i, 10, "Unlock timestamp was not reached");

    // Complete unlock
    let outcome = user
        .call(&lockup_account_id, "end_unlock_near")
        .args_json(json!({ "amount": NearToken::from_near(30) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    outcome_check(&outcome);

    let locked_after_end_unlock = v.get_venear_locked(&lockup_account_id).await?;
    assert_eq!(locked_after_end_unlock, NearToken::from_near(20));
    Ok(())
}

#[tokio::test]

async fn test_over_unlock_should_fail() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_account_id = v.get_lockup_account_id(user.id()).await?;

    transfer_and_lock(&v, &user, NearToken::from_near(100)).await?;

    // Try to unlock 150 NEAR
    let outcome = user
        .call(&lockup_account_id, "begin_unlock_near")
        .args_json(json!({ "amount": NearToken::from_near(150) }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "Should fail when unlocking more than locked"
    );

    Ok(())
}

#[tokio::test]

async fn test_early_unlock_attempt() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_id = v.get_lockup_account_id(user.id()).await?;
    transfer_and_lock(&v, &user, NearToken::from_near(100)).await?;

    user.call(&lockup_id, "begin_unlock_near")
        .args_json(json!({ "amount": NearToken::from_near(100) }))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    // Immediate unlock attempt
    let outcome = user
        .call(&lockup_id, "end_unlock_near")
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(outcome.is_failure(), "Early unlock should be prevented");

    Ok(())
}
