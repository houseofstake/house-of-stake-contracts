mod setup;

use crate::setup::{
    assert_almost_eq, VenearTestWorkspace, VenearTestWorkspaceBuilder, VENEAR_WASM_FILEPATH,
};
use common::{near_add, Fraction};
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

#[tokio::test]
async fn test_venear_growth() -> Result<(), Box<dyn std::error::Error>> {
    // 10 minutes in nanoseconds
    let period = 600 * 10u128.pow(9);
    // Configure the annual growth rate to be 10% per selected period
    let v = VenearTestWorkspaceBuilder::default()
        .annual_growth_rate_ns(Fraction {
            numerator: 10.into(),
            denominator: (100 * period).into(),
        })
        .build()
        .await?;
    let user = v.create_account_with_lockup().await?;
    v.transfer_and_lock(&user, NearToken::from_near(1000))
        .await?;

    let account_info = v.account_info(user.id()).await?;
    let near_balance: NearToken =
        serde_json::from_value(account_info["account"]["balance"]["near_balance"].clone())?;
    // The expected balance is 1000 from lockup + 0.1 from local storage
    let expected_balance = NearToken::from_millinear(1000100);
    assert_eq!(near_balance, expected_balance);
    let balance = v.ft_balance(user.id()).await?;
    assert_almost_eq(balance, expected_balance, NearToken::from_near(1));
    // Account for the growth during the deployment and function calls
    let actual_early_diff = balance.checked_sub(expected_balance).unwrap();
    // println!("Actual diff {}", actual_early_diff.exact_amount_display());

    let start_timestamp = v.sandbox.view_block().await?.timestamp();

    v.fast_forward(
        start_timestamp + period as u64,
        (period / 10u128.pow(9)) as u64 / 5,
        30,
    )
    .await?;

    let timestamp = v.sandbox.view_block().await?.timestamp();
    let balance = v.ft_balance(user.id()).await?;

    let approximate_growth = 0.1f64 * (timestamp - start_timestamp) as f64 / period as f64;
    // println!(
    //     "Time passed {:.3} sec",
    //     (timestamp - start_timestamp) as f64 / 1e9f64
    // );
    // println!("Approximate growth {:.3}", approximate_growth);

    let new_expected_balance = NearToken::from_yoctonear(
        (expected_balance.as_yoctonear() as f64 * (1.0 + approximate_growth)) as _,
    )
    .checked_add(actual_early_diff)
    .unwrap();
    assert_almost_eq(
        balance,
        new_expected_balance,
        NearToken::from_millinear(100),
    );

    Ok(())
}

#[tokio::test]
async fn test_ft_events() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default()
        .annual_growth_rate_ns(Fraction {
            numerator: 0.into(),
            denominator: 1.into(),
        })
        .build()
        .await?;
    let user_a = v.sandbox.dev_create_account().await?;

    let storage_balance_bounds: serde_json::Value = v
        .sandbox
        .view(v.venear.id(), "storage_balance_bounds")
        .await?
        .json()?;

    let storage_balance_bounds_min: NearToken =
        serde_json::from_value(storage_balance_bounds["min"].clone())?;

    let outcome = user_a
        .call(v.venear.id(), "storage_deposit")
        .deposit(storage_balance_bounds_min)
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(outcome.is_success());
    let event = outcome.logs()[0];
    let event: serde_json::Value =
        serde_json::from_str(event.trim_start_matches("EVENT_JSON:")).unwrap();
    assert_eq!(event["standard"].as_str().unwrap(), "nep141");
    assert_eq!(event["event"].as_str().unwrap(), "ft_mint");
    let event_data = &event["data"][0];
    assert_eq!(
        event_data["owner_id"].as_str().unwrap(),
        user_a.id().as_str()
    );
    assert_eq!(
        event_data["amount"].as_str().unwrap(),
        storage_balance_bounds_min.as_yoctonear().to_string()
    );

    let user_b = v.create_account_with_lockup().await?;
    let lockup_id_b = v.get_lockup_account_id(user_b.id()).await?;

    // Lock 1 NEAR
    let outcome = user_b
        .call(&lockup_id_b, "lock_near")
        .args_json(json!({
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success());
    let mut found = false;
    for log in outcome.logs() {
        let event = log;
        let event: serde_json::Value =
            serde_json::from_str(event.trim_start_matches("EVENT_JSON:")).unwrap();
        let standard = event["standard"].as_str().unwrap();
        if standard != "nep141" {
            continue;
        }
        found = true;
        assert_eq!(event["event"].as_str().unwrap(), "ft_mint");
        let event_data = &event["data"][0];
        assert_eq!(
            event_data["owner_id"].as_str().unwrap(),
            user_b.id().as_str()
        );
        assert_eq!(
            event_data["amount"].as_str().unwrap(),
            NearToken::from_near(1).as_yoctonear().to_string()
        );
    }
    assert!(found, "Expected ft_mint event not found");

    let balance_a = v.ft_balance(user_a.id()).await?;
    assert_eq!(balance_a, storage_balance_bounds_min);

    let balance_b = v.ft_balance(user_b.id()).await?;
    // Delegate all from user B to user A
    let outcome = user_b
        .call(v.venear.id(), "delegate_all")
        .args_json(json!({
            "receiver_id": user_a.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(outcome.is_success());

    let mut logs = outcome.logs();
    logs.sort();
    let event = logs[0];
    let event: serde_json::Value =
        serde_json::from_str(event.trim_start_matches("EVENT_JSON:")).unwrap();
    assert_eq!(event["standard"].as_str().unwrap(), "nep141");
    assert_eq!(event["event"].as_str().unwrap(), "ft_burn");
    let event_data = &event["data"][0];
    assert_eq!(
        event_data["owner_id"].as_str().unwrap(),
        user_b.id().as_str()
    );
    assert_eq!(
        event_data["amount"].as_str().unwrap(),
        balance_b.as_yoctonear().to_string()
    );

    let event = logs[1];
    let event: serde_json::Value =
        serde_json::from_str(event.trim_start_matches("EVENT_JSON:")).unwrap();
    assert_eq!(event["standard"].as_str().unwrap(), "nep141");
    assert_eq!(event["event"].as_str().unwrap(), "ft_mint");
    let event_data = &event["data"][0];
    assert_eq!(
        event_data["owner_id"].as_str().unwrap(),
        user_a.id().as_str()
    );
    assert_eq!(
        event_data["amount"].as_str().unwrap(),
        balance_b.as_yoctonear().to_string()
    );

    Ok(())
}
