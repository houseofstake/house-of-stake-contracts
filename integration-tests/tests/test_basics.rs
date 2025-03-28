mod setup;

use crate::setup::{VenearTestWorkspace, VenearTestWorkspaceBuilder, VENEAR_WASM_FILEPATH};
use common::near_add;
use near_sdk::Gas;
use near_workspaces::types::NearToken;
use serde_json::json;

#[tokio::test]
async fn test_deploy_venear_and_account_with_lockup() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let _user = v.create_account_with_lockup().await?;

    Ok(())
}

#[tokio::test]
async fn test_lock_near() -> Result<(), Box<dyn std::error::Error>> {
    let builder = VenearTestWorkspaceBuilder::default();
    let default_balance = builder.local_deposit;
    let v = builder.build().await?;
    let user = v.create_account_with_lockup().await?;

    let account_info = v.account_info(user.id()).await?;

    let balance: NearToken =
        serde_json::from_value(account_info["account"]["balance"]["near_balance"].clone())?;
    let lockup_update_nonce_initial: u64 = account_info["internal"]["lockup_update_nonce"]
        .as_str()
        .unwrap()
        .parse()?;

    assert_eq!(balance, default_balance);

    let lockup_account_id = v.get_lockup_account_id(user.id()).await?;

    let extra_balance = NearToken::from_near(10);
    let outcome = v
        .sandbox
        .root_account()
        .unwrap()
        .transfer_near(&lockup_account_id, extra_balance)
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to transfer NEAR to lockup: {:#?}",
        outcome.outcomes()
    );

    let outcome = user
        .call(&lockup_account_id, "lock_near")
        .args_json(json!({
            "amount": extra_balance
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to lock NEAR: {:#?}",
        outcome.outcomes()
    );

    let account_info = v.account_info(user.id()).await?;
    let balance: NearToken =
        serde_json::from_value(account_info["account"]["balance"]["near_balance"].clone())?;
    assert_eq!(balance, near_add(default_balance, extra_balance));

    let lockup_update_nonce_current: u64 = account_info["internal"]["lockup_update_nonce"]
        .as_str()
        .unwrap()
        .parse()?;
    assert_eq!(
        lockup_update_nonce_initial + 1,
        lockup_update_nonce_current,
        "Lockup update nonce should be incremented"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user_a = v.create_account_with_lockup().await?;

    let temp_user = v.sandbox.dev_create_account().await?;

    let account_info_a = v.account_info(user_a.id()).await?;
    let balance_a: NearToken =
        serde_json::from_value(account_info_a["account"]["balance"]["near_balance"].clone())?;
    assert!(
        account_info_a["account"]["delegation"].is_null(),
        "Delegation should be null"
    );

    let outcome = user_a
        .call(v.venear.id(), "delegate_all")
        .args_json(json!({
            "receiver_id": temp_user.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "Expected failure, but got success: {:#?}",
        outcome.outcomes()
    );

    let user_b = v.create_account_with_lockup().await?;

    let account_info_b = v.account_info(user_b.id()).await?;
    let delegated_balance: NearToken = serde_json::from_value(
        account_info_b["account"]["delegated_balance"]["near_balance"].clone(),
    )?;
    assert!(
        delegated_balance.is_zero(),
        "Delegated balance should be zero"
    );

    let outcome = user_a
        .call(v.venear.id(), "delegate_all")
        .args_json(json!({
            "receiver_id": user_b.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Failed to delegate NEAR: {:#?}",
        outcome.outcomes()
    );

    let account_info_a = v.account_info(user_a.id()).await?;
    assert_eq!(
        account_info_a["account"]["delegation"]["account_id"]
            .as_str()
            .unwrap(),
        user_b.id().as_str(),
        "Delegation account ID should be equal to user B"
    );

    let account_info_b = v.account_info(user_b.id()).await?;
    let delegated_balance: NearToken = serde_json::from_value(
        account_info_b["account"]["delegated_balance"]["near_balance"].clone(),
    )?;
    assert_eq!(
        delegated_balance, balance_a,
        "Delegated balance should be equal to balance from user A"
    );

    Ok(())
}

async fn attempt_venear_upgrade(
    user: &near_workspaces::Account,
    v: &VenearTestWorkspace,
) -> Result<(), Box<dyn std::error::Error>> {
    let venear_wasm = std::fs::read(VENEAR_WASM_FILEPATH)?;

    let outcome = user
        .call(v.venear.id(), "upgrade")
        .args(venear_wasm)
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;

    if !outcome.is_success() {
        return Err(format!(
            "Failed to upgrade venear contract: {:#?}",
            outcome.outcomes()
        )
        .into());
    }

    Ok(())
}

#[tokio::test]
async fn test_upgrade_venear() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user_a = v.create_account_with_lockup().await?;

    assert!(
        attempt_venear_upgrade(&user_a, &v).await.is_err(),
        "User should not be able to upgrade the contract"
    );

    attempt_venear_upgrade(&v.venear_owner, &v).await?;

    Ok(())
}
