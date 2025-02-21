use near_sdk::{AccountId, Gas, NearToken};
use near_workspaces::operations::Function;
use serde_json::json;

const LOCKUP_WASM_FILEPATH: &str = "../res/local/lockup_contract.wasm";
const VENEAR_WASM_FILEPATH: &str = "../res/local/venear_contract.wasm";

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let lockup_wasm = std::fs::read(LOCKUP_WASM_FILEPATH)?;
    let venear_wasm = std::fs::read(VENEAR_WASM_FILEPATH)?;

    let sandbox = near_workspaces::sandbox().await?;

    // TODO: Deploy
    let staking_pool_whitelist_account_id = sandbox.dev_create_account().await?;

    let lockup_deployer = sandbox.dev_create_account().await?;
    let venear_owner = sandbox.dev_create_account().await?;

    let venear = sandbox.dev_create_account().await?;
    let args = json!({
        "config": {
            "lockup_contract_config": None::<String>,
            "lockup_duration_ns": (90u64 * 24 * 60 * 60 * 10u64.pow(9)).to_string(),
            "staking_pool_whitelist_account_id": staking_pool_whitelist_account_id.id(),
            "lockup_code_deployers": &[lockup_deployer.id()],
            "local_deposit": NearToken::from_millinear(100),
            "min_extra_lockup_deposit": NearToken::from_millinear(100),
            "owner_account_id": venear_owner.id(),
        },
        "venear_growth_config": {
            "annual_growth_rate_ns": {
                "numerator": "1",
                "denominator": "1",
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

    let user_account = sandbox.dev_create_account().await?;
    //
    // let outcome = user_account
    //     .call(contract.id(), "set_greeting")
    //     .args_json(json!({"greeting": "Hello World!"}))
    //     .transact()
    //     .await?;
    // assert!(outcome.is_success());
    //
    // let user_message_outcome = contract.view("get_greeting").args_json(json!({})).await?;
    // assert_eq!(user_message_outcome.json::<String>()?, "Hello World!");

    Ok(())
}
