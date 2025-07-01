mod setup;

use crate::setup::{
    assert_almost_eq, VenearTestWorkspace, VenearTestWorkspaceBuilder, VENEAR_WASM_FILEPATH,
};
use common::{near_add, Fraction, TimestampNs};
use near_sdk::json_types::Base58CryptoHash;
use near_sdk::{CryptoHash, Gas};
use near_workspaces::types::NearToken;
use near_workspaces::AccountId;
use serde_json::json;
use sha2::Digest;

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

    // Undelegate
    let outcome = user_a
        .call(v.venear.id(), "undelegate")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to undelegate NEAR: {:#?}",
        outcome.outcomes()
    );

    let account_info_a = v.account_info(user_a.id()).await?;
    assert!(
        account_info_a["account"]["delegation"].is_null(),
        "Delegation should be null"
    );
    let account_info_b = v.account_info(user_b.id()).await?;
    let delegated_balance: NearToken = serde_json::from_value(
        account_info_b["account"]["delegated_balance"]["near_balance"].clone(),
    )?;
    assert_eq!(
        delegated_balance,
        NearToken::from_yoctonear(0),
        "Delegated balance should be zero"
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

#[tokio::test]
async fn test_venear_governance() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;

    let original_config: serde_json::Value =
        v.sandbox.view(v.venear.id(), "get_config").await?.json()?;

    let original_local_deposit: NearToken =
        serde_json::from_value(original_config["local_deposit"].clone())?;
    let new_local_deposit = NearToken::from_yoctonear(1000);
    assert_ne!(original_local_deposit, new_local_deposit);
    let mut venear_owner = v.venear_owner.clone();

    // Attempt set_local_deposit
    let outcome = user
        .call(v.venear.id(), "set_local_deposit")
        .args_json(json!({
            "local_deposit": new_local_deposit
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set local deposit",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let local_deposit: NearToken = serde_json::from_value(config["local_deposit"].clone())?;
    assert_eq!(local_deposit, original_local_deposit);

    let outcome = venear_owner
        .call(v.venear.id(), "set_local_deposit")
        .args_json(json!({
            "local_deposit": new_local_deposit
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set local deposit",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let local_deposit: NearToken = serde_json::from_value(config["local_deposit"].clone())?;
    assert_eq!(local_deposit, new_local_deposit);

    // staking_pool_whitelist_account_id

    let original_staking_pool_whitelist_account_id: AccountId =
        serde_json::from_value(original_config["staking_pool_whitelist_account_id"].clone())?;
    let new_staking_pool_whitelist_account_id: AccountId =
        "new_staking_pool_whitelist_account_id.near".parse()?;
    assert_ne!(
        original_staking_pool_whitelist_account_id,
        new_staking_pool_whitelist_account_id
    );

    // Attempt set_staking_pool_whitelist_account_id
    let outcome = user
        .call(v.venear.id(), "set_staking_pool_whitelist_account_id")
        .args_json(json!({
            "staking_pool_whitelist_account_id": new_staking_pool_whitelist_account_id
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set staking_pool_whitelist_account_id",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let staking_pool_whitelist_account_id: AccountId =
        serde_json::from_value(config["staking_pool_whitelist_account_id"].clone())?;
    assert_eq!(
        staking_pool_whitelist_account_id,
        original_staking_pool_whitelist_account_id
    );

    let outcome = venear_owner
        .call(v.venear.id(), "set_staking_pool_whitelist_account_id")
        .args_json(json!({
            "staking_pool_whitelist_account_id": new_staking_pool_whitelist_account_id
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set staking_pool_whitelist_account_id",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let staking_pool_whitelist_account_id: AccountId =
        serde_json::from_value(config["staking_pool_whitelist_account_id"].clone())?;
    assert_eq!(
        staking_pool_whitelist_account_id,
        new_staking_pool_whitelist_account_id
    );

    // unlock_duration_ns

    let original_unlock_duration_ns: TimestampNs =
        serde_json::from_value(original_config["unlock_duration_ns"].clone())?;
    let new_unlock_duration_sec: u32 = 100;
    let new_unlock_duration_ns: TimestampNs =
        (new_unlock_duration_sec as u64 * 10u64.pow(9)).into();
    assert_ne!(original_unlock_duration_ns, new_unlock_duration_ns);

    // Attempt set_unlock_duration_ns
    let outcome = user
        .call(v.venear.id(), "set_unlock_duration_sec")
        .args_json(json!({
            "unlock_duration_sec": new_unlock_duration_sec
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set unlock_duration_ns",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let unlock_duration_ns: TimestampNs =
        serde_json::from_value(config["unlock_duration_ns"].clone())?;
    assert_eq!(unlock_duration_ns, original_unlock_duration_ns);

    let outcome = venear_owner
        .call(v.venear.id(), "set_unlock_duration_sec")
        .args_json(json!({
            "unlock_duration_sec": new_unlock_duration_sec
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set unlock_duration_ns",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let unlock_duration_ns: TimestampNs =
        serde_json::from_value(config["unlock_duration_ns"].clone())?;
    assert_eq!(unlock_duration_ns, new_unlock_duration_ns);

    // Lockup contract code deployers

    let original_lockup_code_deployers: Vec<AccountId> =
        serde_json::from_value(original_config["lockup_code_deployers"].clone())?;
    let new_lockup_deployer = v.sandbox.dev_create_account().await?;

    let new_lockup_code_deployers: Vec<AccountId> = vec![
        "new_lockup_code_deployer_1.near".parse()?,
        new_lockup_deployer.id().clone(),
    ];
    assert_ne!(original_lockup_code_deployers, new_lockup_code_deployers);

    // Attempt set_lockup_code_deployers
    let outcome = user
        .call(v.venear.id(), "set_lockup_code_deployers")
        .args_json(json!({
            "lockup_code_deployers": new_lockup_code_deployers
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set lockup_code_deployers",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let lockup_code_deployers: Vec<AccountId> =
        serde_json::from_value(config["lockup_code_deployers"].clone())?;
    assert_eq!(lockup_code_deployers, original_lockup_code_deployers);

    let outcome = venear_owner
        .call(v.venear.id(), "set_lockup_code_deployers")
        .args_json(json!({
            "lockup_code_deployers": new_lockup_code_deployers
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set lockup_code_deployers",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let lockup_code_deployers: Vec<AccountId> =
        serde_json::from_value(config["lockup_code_deployers"].clone())?;
    assert_eq!(lockup_code_deployers, new_lockup_code_deployers);

    // Guardians

    let original_guardians: Vec<AccountId> =
        serde_json::from_value(original_config["guardians"].clone())?;
    let new_guardian = v.sandbox.dev_create_account().await?;

    let new_guardians: Vec<AccountId> =
        vec!["new_guardian_1.near".parse()?, new_guardian.id().clone()];
    assert_ne!(original_guardians, new_guardians);

    // Attempt set_guardians
    let outcome = user
        .call(v.venear.id(), "set_guardians")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let guardians: Vec<AccountId> = serde_json::from_value(config["guardians"].clone())?;
    assert_eq!(guardians, original_guardians);

    let outcome = venear_owner
        .call(v.venear.id(), "set_guardians")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let guardians: Vec<AccountId> = serde_json::from_value(config["guardians"].clone())?;
    assert_eq!(guardians, new_guardians);

    // propose_new_owner_account_id
    let new_owner_account = v.sandbox.dev_create_account().await?;
    let original_owner_account_id: AccountId =
        serde_json::from_value(original_config["owner_account_id"].clone())?;
    let new_owner_account_id: AccountId = new_owner_account.id().clone();
    assert_ne!(original_owner_account_id, new_owner_account_id);

    // Attempt propose_new_owner_account_id
    let outcome = user
        .call(v.venear.id(), "propose_new_owner_account_id")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    let outcome = venear_owner
        .call(v.venear.id(), "propose_new_owner_account_id")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert_eq!(
        proposed_new_owner_account_id.as_ref(),
        Some(&new_owner_account_id)
    );

    // Cancel proposal
    let outcome = venear_owner
        .call(v.venear.id(), "propose_new_owner_account_id")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, original_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    let outcome = venear_owner
        .call(v.venear.id(), "propose_new_owner_account_id")
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

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
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
        .call(v.venear.id(), "accept_ownership")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to accept the ownership",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
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
        .call(v.venear.id(), "accept_ownership")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "The new owner should be able to accept the ownership",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let owner_account_id: AccountId = serde_json::from_value(config["owner_account_id"].clone())?;
    assert_eq!(owner_account_id, new_owner_account_id);
    let proposed_new_owner_account_id: Option<AccountId> =
        serde_json::from_value(config["proposed_new_owner_account_id"].clone())?;
    assert!(proposed_new_owner_account_id.is_none());

    venear_owner = new_owner_account;

    // Deploy new lockup
    let new_lockup_wasm = b"yolo".to_vec();
    let new_lockup_contract_hash: CryptoHash = sha2::Sha256::digest(&new_lockup_wasm).into();
    let new_lockup_contract_hash: Base58CryptoHash =
        Base58CryptoHash::try_from(new_lockup_contract_hash).unwrap();

    let original_lockup_contract_hash: Base58CryptoHash =
        serde_json::from_value(original_config["lockup_contract_config"]["contract_hash"].clone())?;
    assert_ne!(original_lockup_contract_hash, new_lockup_contract_hash);

    // Attempt to prepare the new lockup contract
    let outcome = user
        .call(v.venear.id(), "prepare_lockup_code")
        .args(new_lockup_wasm.clone())
        .deposit(NearToken::from_near(2))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "User should not be able to prepare the new lockup contract",
    );

    // Attempt to prepare the new lockup contract from the original lockup deployer
    let outcome = v
        .lockup_deployer
        .call(v.venear.id(), "prepare_lockup_code")
        .args(new_lockup_wasm.clone())
        .deposit(NearToken::from_near(2))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Original lockup deployer should not be able to prepare the new lockup contract",
    );

    // Prepare the new lockup contract from the new lockup deployer
    let outcome = new_lockup_deployer
        .call(v.venear.id(), "prepare_lockup_code")
        .args(new_lockup_wasm)
        .deposit(NearToken::from_near(2))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "New lockup deployer should be able to prepare the new lockup contract",
    );

    let contract_hash: Base58CryptoHash = outcome.unwrap().json()?;
    assert_eq!(
        contract_hash, new_lockup_contract_hash,
        "Invalid contract hash"
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let lockup_contract_hash: Base58CryptoHash =
        serde_json::from_value(config["lockup_contract_config"]["contract_hash"].clone())?;
    assert_ne!(
        lockup_contract_hash, new_lockup_contract_hash,
        "The lockup contract hash should be updated automatically"
    );

    let original_min_lockup_deposit: NearToken =
        serde_json::from_value(config["min_lockup_deposit"].clone())?;
    let new_min_lockup_deposit = NearToken::from_near(3);
    assert_ne!(original_min_lockup_deposit, new_min_lockup_deposit);

    let original_contract_version: u64 =
        serde_json::from_value(config["lockup_contract_config"]["contract_version"].clone())?;
    assert_eq!(original_contract_version, 1);

    // Attempt to change the lockup contract hash by the user
    let outcome = user
        .call(v.venear.id(), "set_lockup_contract")
        .args_json(json!({
            "contract_hash": new_lockup_contract_hash,
            "min_lockup_deposit": new_min_lockup_deposit,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to set the lockup contract hash",
    );

    // Attempt to change the lockup contract hash by the lockup deployer
    let outcome = new_lockup_deployer
        .call(v.venear.id(), "set_lockup_contract")
        .args_json(json!({
            "contract_hash": new_lockup_contract_hash,
            "min_lockup_deposit": new_min_lockup_deposit,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Lockup deployer should not be able to set the lockup contract hash",
    );

    // Change the lockup contract hash by the owner
    let outcome = venear_owner
        .call(v.venear.id(), "set_lockup_contract")
        .args_json(json!({
            "contract_hash": new_lockup_contract_hash,
            "min_lockup_deposit": new_min_lockup_deposit,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Owner should be able to set the lockup contract hash",
    );

    let config: serde_json::Value = v.sandbox.view(v.venear.id(), "get_config").await?.json()?;
    let min_lockup_deposit: NearToken =
        serde_json::from_value(config["min_lockup_deposit"].clone())?;
    assert_eq!(min_lockup_deposit, new_min_lockup_deposit);

    let lockup_contract_hash: Base58CryptoHash =
        serde_json::from_value(config["lockup_contract_config"]["contract_hash"].clone())?;
    assert_eq!(
        lockup_contract_hash, new_lockup_contract_hash,
        "The lockup contract hash should be updated"
    );

    let contract_version: u64 =
        serde_json::from_value(config["lockup_contract_config"]["contract_version"].clone())?;
    assert_eq!(
        contract_version, 2,
        "The lockup contract version should be updated"
    );

    Ok(())
}

#[tokio::test]
async fn test_venear_pause() -> Result<(), Box<dyn std::error::Error>> {
    let v = VenearTestWorkspaceBuilder::default().build().await?;
    let user = v.create_account_with_lockup().await?;
    let user_2 = v.sandbox.dev_create_account().await?;

    // Attempt to create user 3 account
    let storage_balance_bounds: serde_json::Value = v
        .sandbox
        .view(v.venear.id(), "storage_balance_bounds")
        .await?
        .json()?;

    let storage_balance_bounds_min: u128 =
        storage_balance_bounds["min"].as_str().unwrap().parse()?;

    let outcome = user_2
        .call(v.venear.id(), "storage_deposit")
        .deposit(NearToken::from_yoctonear(storage_balance_bounds_min))
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to do storage_deposit: {:#?}",
        outcome.outcomes()
    );

    let account_info = v.account_info(user_2.id()).await?;
    assert!(!account_info.is_null(), "Account should be registered");

    // delegate_all to user_2
    let outcome = user
        .call(v.venear.id(), "delegate_all")
        .args_json(json!({
            "receiver_id": user_2.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to delegate_all: {:#?}",
        outcome.outcomes()
    );

    // Attempt to pause the contract
    let outcome = user
        .call(v.venear.id(), "pause")
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
        .view(v.venear.id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(!is_paused, "Contract should not be paused");

    // Pause the contract by the guardian
    let outcome = v
        .guardian
        .call(v.venear.id(), "pause")
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
        .view(v.venear.id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(is_paused, "Contract should be paused");

    // Check if guardian can unpause the contract
    let outcome = v
        .guardian
        .call(v.venear.id(), "unpause")
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
        .view(v.venear.id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(is_paused, "Contract should be paused");

    // Unpause the contract by the owner
    let outcome = v
        .venear_owner
        .call(v.venear.id(), "unpause")
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
        .view(v.venear.id(), "is_paused")
        .await?
        .json()
        .unwrap();
    assert!(!is_paused, "Contract should not be paused");

    // Pause the contract by the owner
    let outcome = v
        .venear_owner
        .call(v.venear.id(), "pause")
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
        .view(v.venear.id(), "is_paused")
        .await?
        .json()
        .unwrap();

    assert!(is_paused, "Contract should be paused");

    // Testing paused methods
    let user_3 = v.sandbox.dev_create_account().await?;

    // Attempt to create user 3 account
    let outcome = user_3
        .call(v.venear.id(), "storage_deposit")
        .deposit(NearToken::from_yoctonear(storage_balance_bounds_min))
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to create an account when the contract is paused",
    );

    // Attempt to undelegate_all
    let outcome = user_2
        .call(v.venear.id(), "undelegate")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to undelegate when the contract is paused",
    );

    // Attempt to delegate_all
    let outcome = user_2
        .call(v.venear.id(), "delegate_all")
        .args_json(json!({
            "receiver_id": user.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to delegate when the contract is paused",
    );

    // Attempt to deploy a new lockup
    let lockup_cost: NearToken = v
        .sandbox
        .view(v.venear.id(), "get_lockup_deployment_cost")
        .await?
        .json()?;

    let outcome = user_2
        .call(v.venear.id(), "deploy_lockup")
        .args_json(json!({}))
        .deposit(lockup_cost)
        .gas(Gas::from_tgas(200))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "User should not be able to deploy a new lockup when the contract is paused",
    );

    // Attempt to get snapshot
    assert!(
        v.sandbox.view(v.venear.id(), "get_snapshot").await.is_err(),
        "The contract should not be able to get snapshot when paused"
    );

    // Attempt to get proof
    assert!(
        v.sandbox
            .view(v.venear.id(), "get_proof")
            .args_json(json!({
                "account_id": user.id()
            }))
            .await
            .is_err(),
        "The contract should not be able to get proof when paused"
    );

    Ok(())
}
