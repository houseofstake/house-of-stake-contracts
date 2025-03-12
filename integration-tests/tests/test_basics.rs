use near_sdk::json_types::Base58CryptoHash;
use near_sdk::{CryptoHash, Gas, NearToken};
use near_workspaces::operations::Function;
use near_workspaces::AccountId;
use serde_json::json;
use sha2::Digest;
use std::str::FromStr;

const LOCKUP_WASM_FILEPATH: &str = "../res/local/lockup_contract.wasm";
const VENEAR_WASM_FILEPATH: &str = "../res/local/venear_contract.wasm";

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let lockup_wasm = std::fs::read(LOCKUP_WASM_FILEPATH)?;
    let lockup_hash: CryptoHash = sha2::Sha256::digest(&lockup_wasm).into();
    let lockup_hash = Base58CryptoHash::from(lockup_hash);

    let venear_wasm = std::fs::read(VENEAR_WASM_FILEPATH)?;

    let sandbox = near_workspaces::sandbox().await?;

    // TODO: Deploy
    let staking_pool_whitelist_account_id = sandbox.dev_create_account().await?;

    let lockup_deployer = sandbox.dev_create_account().await?;
    let venear_owner = sandbox.dev_create_account().await?;

    let venear = sandbox.dev_create_account().await?;
    // Need a shorter name, otherwise the lockup hash will not fit into 64 bytes
    let venear = sandbox
        .create_root_account_subaccount(
            AccountId::from_str("venear").unwrap(),
            venear.secret_key().clone(),
        )
        .await?
        .unwrap();

    let local_deposit = NearToken::from_millinear(100);
    let min_extra_lockup_deposit = NearToken::from_millinear(200);

    let args = json!({
        "config": {
            "lockup_contract_config": None::<String>,
            "lockup_duration_ns": (90u64 * 24 * 60 * 60 * 10u64.pow(9)).to_string(),
            "staking_pool_whitelist_account_id": staking_pool_whitelist_account_id.id(),
            "lockup_code_deployers": &[lockup_deployer.id()],
            "local_deposit": local_deposit,
            "min_extra_lockup_deposit": min_extra_lockup_deposit,
            "owner_account_id": venear_owner.id(),
        },
        "venear_growth_config": {
            "annual_growth_rate_ns": {
                "numerator": "6",
                "denominator": (365u64 * 24 * 60 * 60 * 10u64.pow(9)).to_string(),
            }
        },
    });

    let outcome = venear
        .batch(venear.id())
        .deploy(&venear_wasm)
        .call(
            Function::new("init")
                .args_json(args)
                .gas(Gas::from_tgas(10)),
        )
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to deploy venear: {:#?}",
        outcome.outcomes()
    );

    // Adding lockup contract

    let outcome = lockup_deployer
        .call(venear.id(), "prepare_lockup_code")
        .args(lockup_wasm)
        .deposit(NearToken::from_near(2))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Failed to add lockup code to venear: {:#?}",
        outcome.outcomes()
    );

    let contract_hash: Base58CryptoHash = outcome.unwrap().json()?;
    assert_eq!(contract_hash, lockup_hash, "Invalid contract hash");

    let outcome = venear_owner
        .call(venear.id(), "set_lockup_contract")
        .args_json(json!({
            "contract_hash": contract_hash,
            "min_extra_lockup_deposit": min_extra_lockup_deposit,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Failed to set lockup contract on venear: {:#?}",
        outcome.outcomes()
    );

    let user_account = sandbox.dev_create_account().await?;

    let account_info: serde_json::Value = sandbox
        .view(venear.id(), "get_account_info")
        .args_json(json!({
            "account_id": user_account.id(),
        }))
        .await
        .unwrap()
        .json()
        .unwrap();
    assert!(account_info.is_null(), "Account should not be registered");

    let storage_balance_bounds: serde_json::Value = sandbox
        .view(venear.id(), "storage_balance_bounds")
        .await
        .unwrap()
        .json()
        .unwrap();

    let storage_balance_bounds_min: u128 = storage_balance_bounds["min"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        storage_balance_bounds_min,
        local_deposit.as_yoctonear(),
        "Invalid storage balance bounds"
    );

    // Attempt to register account with less funding
    let outcome = user_account
        .call(venear.id(), "storage_deposit")
        .deposit(NearToken::from_yoctonear(storage_balance_bounds_min - 1))
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(
        outcome.is_failure(),
        "Expected to fail on storage_deposit with less funds: {:#?}",
        outcome.outcomes()
    );

    let account_info: serde_json::Value = sandbox
        .view(venear.id(), "get_account_info")
        .args_json(json!({
            "account_id": user_account.id(),
        }))
        .await
        .unwrap()
        .json()
        .unwrap();
    assert!(account_info.is_null(), "Account should not be registered");

    let outcome = user_account
        .call(venear.id(), "storage_deposit")
        .deposit(NearToken::from_yoctonear(storage_balance_bounds_min))
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Failed to do storage_deposit: {:#?}",
        outcome.outcomes()
    );

    let account_info: serde_json::Value = sandbox
        .view(venear.id(), "get_account_info")
        .args_json(json!({
            "account_id": user_account.id(),
        }))
        .await
        .unwrap()
        .json()
        .unwrap();
    println!("{:#?}", account_info);

    Ok(())
}
