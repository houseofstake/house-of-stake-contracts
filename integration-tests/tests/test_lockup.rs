mod setup;

use crate::setup::{
    assert_almost_eq, outcome_check, VenearTestWorkspace, VenearTestWorkspaceBuilder,
    UNLOCK_DURATION_SECONDS,
};
use near_sdk::Gas;
use near_workspaces::types::NearToken;
use near_workspaces::{Account, AccountId};
use serde_json::json;

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

    v.fast_forward(unlock_timestamp, UNLOCK_DURATION_SECONDS, 10)
        .await?;

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

    v.transfer_and_lock(&user, NearToken::from_near(100))
        .await?;

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
    v.transfer_and_lock(&user, NearToken::from_near(100))
        .await?;

    let outcome = user
        .call(&lockup_id, "begin_unlock_near")
        .args_json(json!({ "amount": NearToken::from_near(100) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(outcome.is_success(), "Unlock should be successful");

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

async fn attempt_lockup_delete(
    v: &VenearTestWorkspace,
    user: &Account,
) -> Result<(), Box<dyn std::error::Error>> {
    let lockup_id = v.get_lockup_account_id(user.id()).await?;
    let outcome = user
        .call(&lockup_id, "delete_lockup")
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    if outcome.is_failure() {
        return Err(format!("Failed to delete lockup: {:#?}", outcome).into());
    }

    Ok(())
}

#[tokio::test]
pub async fn test_lockup_recreation() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_id = v.get_lockup_account_id(user.id()).await?;
    v.transfer_and_lock(&user, NearToken::from_near(100))
        .await?;

    assert!(
        v.sandbox.view_account(&lockup_id).await.is_ok(),
        "Lockup account should exist"
    );

    // Attempt to delete the lockup account, but it should fail because of locked NEAR
    assert!(
        attempt_lockup_delete(&v, &user).await.is_err(),
        "Lockup deletion should fail"
    );

    assert!(
        v.sandbox.view_account(&lockup_id).await.is_ok(),
        "Lockup account should exist"
    );

    let outcome = user
        .call(&lockup_id, "begin_unlock_near")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(outcome.is_success(), "Unlock should be successful");

    assert_eq!(
        v.get_venear_pending(&lockup_id).await?,
        NearToken::from_near(100),
        "Pending should be 100 NEAR"
    );

    let unlock_timestamp = v.get_venear_unlock_timestamp(&lockup_id).await?;
    assert!(unlock_timestamp > 0, "venear_unlock_timestamp was not set");

    v.fast_forward(unlock_timestamp, UNLOCK_DURATION_SECONDS, 10)
        .await?;

    // Complete unlock
    let outcome = user
        .call(&lockup_id, "end_unlock_near")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success(), "Unlock should be successful");

    // Attempt to delete the lockup account again, this time it should succeed
    attempt_lockup_delete(&v, &user).await?;

    // Check that the lockup account is deleted
    assert!(
        v.sandbox.view_account(&lockup_id).await.is_err(),
        "Lockup account should be deleted"
    );

    // Redeploy lockup

    let lockup_cost: NearToken = v
        .sandbox
        .view(v.venear.id(), "get_lockup_deployment_cost")
        .await?
        .json()?;

    let outcome = user
        .call(v.venear.id(), "deploy_lockup")
        .deposit(lockup_cost)
        .args_json(json!({}))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Lockup deployment should be successful"
    );

    // Check that the lockup account is recreated
    assert!(
        v.sandbox.view_account(&lockup_id).await.is_ok(),
        "Lockup account should exist"
    );

    Ok(())
}

#[tokio::test]
pub async fn test_lockup_staking() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_id = v.get_lockup_account_id(user.id()).await?;

    // Adding NEAR to the lockup account
    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(&lockup_id, NearToken::from_near(60))
        .await?;
    assert!(
        outcome.is_success(),
        "Transfer to lockup account should be successful"
    );

    // Should fail, because the staking pool is not selected
    let outcome = user
        .call(&lockup_id, "deposit_and_stake")
        .args_json(json!({ "amount": NearToken::from_near(50) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Deposit and stake should fail without a staking pool"
    );

    let fake_pool_id = v.sandbox.dev_create_account().await?;

    // Attempt to select non-whitelisted staking pool
    let outcome = user
        .call(&lockup_id, "select_staking_pool")
        .args_json(json!({ "staking_pool_account_id": fake_pool_id.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Selecting non-whitelisted staking pool should fail"
    );

    let selected_staking_pool_id: Option<AccountId> = v
        .sandbox
        .view(&lockup_id, "get_staking_pool_account_id")
        .await?
        .json()?;

    assert!(
        selected_staking_pool_id.is_none(),
        "Staking pool should not be set"
    );

    // Select a whitelisted staking pool
    let outcome = user
        .call(&lockup_id, "select_staking_pool")
        .args_json(json!({ "staking_pool_account_id": v.staking_pool.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Selecting whitelisted staking pool should be successful"
    );

    let selected_staking_pool_id: Option<AccountId> = v
        .sandbox
        .view(&lockup_id, "get_staking_pool_account_id")
        .await?
        .json()?;

    assert_eq!(
        selected_staking_pool_id.as_ref(),
        Some(v.staking_pool.id()),
        "Staking pool should be set correctly"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;

    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(0),
        "Known deposited balance should be 0 NEAR"
    );

    let account_details = v.sandbox.view_account(&lockup_id).await?;
    let initial_balance = account_details.balance;

    // Deposit and stake 50 NEAR
    let outcome = user
        .call(&lockup_id, "deposit_and_stake")
        .args_json(json!({ "amount": NearToken::from_near(50) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Deposit and stake should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(50),
        "Known deposited balance should be 50 NEAR"
    );

    let account_details = v.sandbox.view_account(&lockup_id).await?;
    assert_almost_eq(
        account_details.balance,
        initial_balance
            .checked_sub(NearToken::from_near(50))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    // Verify staked amount
    let staked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_staked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        staked_amount,
        NearToken::from_near(50),
        "Staked amount should be 50 NEAR"
    );

    // Attempt to delete the lockup account
    let outcome = user
        .call(&lockup_id, "delete_lockup")
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Lockup deletion should fail when staking pool is selected"
    );

    // Attempt to lock 30 NEAR (which is more than account balance)
    let initial_lockup_balance = account_details.balance;
    assert!(initial_lockup_balance < NearToken::from_near(30));
    let outcome = user
        .call(&lockup_id, "lock_near")
        .args_json(json!({ "amount": NearToken::from_near(30) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(outcome.is_success(), "Locking 30 NEAR should be successful");

    let user_account_details = v.sandbox.view_account(user.id()).await?;
    let initial_user_balance = user_account_details.balance;

    // Transferring 5 NEAR from the lockup account to user
    let outcome = user
        .call(&lockup_id, "transfer")
        .args_json(json!({ "amount": NearToken::from_near(5), "receiver_id": user.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Transfer from lockup account to user should be successful"
    );

    let user_account_details = v.sandbox.view_account(user.id()).await?;
    assert_almost_eq(
        user_account_details.balance,
        initial_user_balance
            .checked_add(NearToken::from_near(5))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    let lockup_account_details = v.sandbox.view_account(&lockup_id).await?;
    assert_almost_eq(
        lockup_account_details.balance,
        initial_lockup_balance
            .checked_sub(NearToken::from_near(5))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    let initial_lockup_balance = lockup_account_details.balance;

    // Adding NEAR to the staking pool account
    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(v.staking_pool.id(), NearToken::from_near(50))
        .await?;
    assert!(
        outcome.is_success(),
        "Transfer to staking pool account should be successful"
    );

    // Increase lockup balance on staking pool
    let outcome = v
        .staking_pool
        .call(v.staking_pool.id(), "sandbox_update_account")
        .args_json(json!({
            "account_id": lockup_id,
            "account": {
                "staked_balance": NearToken::from_near(100),
                "unstaked_balance": NearToken::from_near(0),
                "can_withdraw": false,
            }
        }))
        .gas(Gas::from_tgas(10))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Updating lockup account on staking pool should be successful"
    );

    // Refresh staking pool balance
    let outcome = user
        .call(&lockup_id, "refresh_staking_pool_balance")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Refreshing staking pool balance should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(100),
        "Known deposited balance should be 100 NEAR"
    );

    // Start unstaking process of 60 NEAR
    let outcome = user
        .call(&lockup_id, "unstake")
        .args_json(json!({ "amount": NearToken::from_near(60) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Unstaking 60 NEAR should be successful"
    );

    // Verify staked amount
    let staking_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_staked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        staking_amount,
        NearToken::from_near(40),
        "Staked amount should be 40 NEAR"
    );

    // Verify unstaked amount
    let unstaked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_unstaked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        unstaked_amount,
        NearToken::from_near(60),
        "Unstaked amount should be 60 NEAR"
    );

    // Refresh staking pool balance
    let outcome = user
        .call(&lockup_id, "refresh_staking_pool_balance")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Refreshing staking pool balance should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(100),
        "Known deposited balance should be 100 NEAR"
    );

    // Update staking pool to allow withdrawal
    let outcome = v
        .staking_pool
        .call(v.staking_pool.id(), "sandbox_update_account")
        .args_json(json!({
            "account_id": lockup_id,
            "account": {
                "staked_balance": NearToken::from_near(40),
                "unstaked_balance": NearToken::from_near(60),
                "can_withdraw": true,
            }
        }))
        .gas(Gas::from_tgas(10))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Updating lockup account on staking pool should be successful"
    );

    // Withdraw 25 NEAR
    let outcome = user
        .call(&lockup_id, "withdraw_from_staking_pool")
        .args_json(json!({ "amount": NearToken::from_near(25) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Withdrawing 25 NEAR should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(75),
        "Known deposited balance should be 75 NEAR"
    );

    let account_details = v.sandbox.view_account(&lockup_id).await?;
    assert_almost_eq(
        account_details.balance,
        initial_lockup_balance
            .checked_add(NearToken::from_near(25))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    let initial_lockup_balance = account_details.balance;

    let unstaked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_unstaked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        unstaked_amount,
        NearToken::from_near(35),
        "Unstaked amount should be 35 NEAR"
    );

    // Withdraw all remaining unstaked NEAR
    let outcome = user
        .call(&lockup_id, "withdraw_all_from_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Withdrawing all unstaked NEAR should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(40),
        "Known deposited balance should be 40 NEAR"
    );

    let account_details = v.sandbox.view_account(&lockup_id).await?;
    assert_almost_eq(
        account_details.balance,
        initial_lockup_balance
            .checked_add(NearToken::from_near(35))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    let initial_lockup_balance = account_details.balance;

    let unstaked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_unstaked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        unstaked_amount,
        NearToken::from_near(0),
        "Unstaked amount should be 0 NEAR"
    );

    // Attempt to unselect the staking pool
    let outcome = user
        .call(&lockup_id, "unselect_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Unselecting staking pool should fail when there are NEAR in the pool"
    );

    // Unstake all NEAR
    let outcome = user
        .call(&lockup_id, "unstake_all")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Withdrawing all unstaked NEAR should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(40),
        "Known deposited balance should be 40 NEAR"
    );

    let unstaked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_unstaked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        unstaked_amount,
        NearToken::from_near(40),
        "Unstaked amount should be 40 NEAR"
    );

    // Verify staked amount
    let staked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_staked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        staked_amount,
        NearToken::from_near(0),
        "Staked amount should be 0 NEAR"
    );

    // Modify the staking pool account to allow withdrawal
    let outcome = v
        .staking_pool
        .call(v.staking_pool.id(), "sandbox_update_account")
        .args_json(json!({
            "account_id": lockup_id,
            "account": {
                "staked_balance": NearToken::from_near(0),
                "unstaked_balance": NearToken::from_near(40),
                "can_withdraw": true,
            }
        }))
        .gas(Gas::from_tgas(10))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Updating lockup account on staking pool should be successful"
    );

    // Withdraw all NEAR
    let outcome = user
        .call(&lockup_id, "withdraw_all_from_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Withdrawing all unstaked NEAR should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(0),
        "Known deposited balance should be 0 NEAR"
    );

    let account_details = v.sandbox.view_account(&lockup_id).await?;
    assert_almost_eq(
        account_details.balance,
        initial_lockup_balance
            .checked_add(NearToken::from_near(40))
            .unwrap(),
        NearToken::from_millinear(1),
    );

    // Unselect the staking pool
    let outcome = user
        .call(&lockup_id, "unselect_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Unselecting staking pool should be successful"
    );

    let selected_staking_pool_id: Option<AccountId> = v
        .sandbox
        .view(&lockup_id, "get_staking_pool_account_id")
        .await?
        .json()?;
    assert!(
        selected_staking_pool_id.is_none(),
        "Staking pool should not be set"
    );

    Ok(())
}

#[tokio::test]
pub async fn test_lockup_delete_after_staking() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let lockup_id = v.get_lockup_account_id(user.id()).await?;

    // Adding NEAR to the lockup account
    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(&lockup_id, NearToken::from_near(60))
        .await?;
    assert!(
        outcome.is_success(),
        "Transfer to lockup account should be successful"
    );

    let outcome = user
        .call(&lockup_id, "select_staking_pool")
        .args_json(json!({ "staking_pool_account_id": v.staking_pool.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Selecting whitelisted staking pool should be successful"
    );

    let selected_staking_pool_id: Option<AccountId> = v
        .sandbox
        .view(&lockup_id, "get_staking_pool_account_id")
        .await?
        .json()?;

    assert_eq!(
        selected_staking_pool_id.as_ref(),
        Some(v.staking_pool.id()),
        "Staking pool should be set correctly"
    );

    // Deposit to the staking pool
    let outcome = user
        .call(&lockup_id, "deposit_to_staking_pool")
        .args_json(json!({ "amount": NearToken::from_near(50) }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Deposit to staking pool should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;

    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(50),
        "Known deposited balance should be 50 NEAR"
    );

    let unstaked_amount: NearToken = v
        .sandbox
        .view(v.staking_pool.id(), "get_account_unstaked_balance")
        .args_json(json!({ "account_id": lockup_id }))
        .await?
        .json()?;
    assert_eq!(
        unstaked_amount,
        NearToken::from_near(50),
        "Unstaked amount should be 50 NEAR"
    );

    // Withdraw all NEAR
    let outcome = user
        .call(&lockup_id, "withdraw_all_from_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Withdrawing all unstaked NEAR should be successful"
    );

    let known_deposited_balance: NearToken = v
        .sandbox
        .view(&lockup_id, "get_known_deposited_balance")
        .await?
        .json()?;
    assert_eq!(
        known_deposited_balance,
        NearToken::from_near(0),
        "Known deposited balance should be 0 NEAR"
    );

    // Unselect the staking pool
    let outcome = user
        .call(&lockup_id, "unselect_staking_pool")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Unselecting staking pool should be successful"
    );

    let selected_staking_pool_id: Option<AccountId> = v
        .sandbox
        .view(&lockup_id, "get_staking_pool_account_id")
        .await?
        .json()?;
    assert!(
        selected_staking_pool_id.is_none(),
        "Staking pool should not be set"
    );

    // Attempt to delete the lockup account
    let outcome = user
        .call(&lockup_id, "delete_lockup")
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success(), "Lockup deletion should be successful");
    assert!(
        v.sandbox.view_account(&lockup_id).await.is_err(),
        "Lockup account should be deleted"
    );

    Ok(())
}
