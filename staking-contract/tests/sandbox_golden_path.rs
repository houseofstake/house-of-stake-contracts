//! Documented golden-path sandbox E2E for one-off catalog locks:
//!
//! `lock` → `epoch_settle` (stake on pool) → wait for `lock.end_ns` → `unlock` →
//! two `epoch_settle` passes (pool unstake + withdraw into contract) → `withdraw(validator_id)` → buyer receives NEAR.
//!
//! Build: `make staking-contract-test mock-staking-pool-contract` (from repo root).
//! Run: `cargo test -p staking-contract --test sandbox_golden_path`

mod mock_pool;

use mock_pool::{
    buyer_lock_one_off, buyer_stake_farm, buyer_storage_deposit, buyer_unstake_farm,
    buyer_withdraw, create_farm_product_and_price, drive_validator_settlement_epochs,
    fast_forward_until_epoch_delta, fetch_validator, json_near_token_yocto, json_tx_status,
    setup_staking_fixture, unlock_lock_after_expiry,
};
use near_workspaces::types::NearToken;
use serde_json::json;

const SHORT_LOCK_NS: &str = "1000000000";
const FARM_REWARD_RATE: &str = "1000000000000000000000000";
const ONE_NEAR_YOCTO: &str = "1000000000000000000000000";
const FOUR_NEAR_YOCTO: u128 = 4_000_000_000_000_000_000_000_000;

fn json_u128_string(v: &serde_json::Value, field: &str) -> u128 {
    v[field]
        .as_str()
        .and_then(|s| s.parse::<u128>().ok())
        .unwrap_or(0)
}

/// End-to-end exit path for a single one-off lock (see module docs).
#[tokio::test]
async fn golden_path_lock_settle_unlock_withdraw_pays_buyer()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    // Step 1 — stake queued NEAR from the lock onto the mock pool.
    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 1).await?;
    let v_staked = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_staked["pending_to_stake"]).unwrap_or(0),
        0,
        "epoch_settle should move lock stake onto the pool"
    );
    assert_eq!(json_tx_status(&v_staked["tx_status"]), Some("Idle"));

    // Step 2 — unlock after lock expiry (queues pool unstake).
    unlock_lock_after_expiry(&worker, &buyer, staking.id(), &lock_id).await?;
    let lock_unlocked: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    assert_eq!(lock_unlocked["status"], json!("UnlockRequested"));
    let v_unlock = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert!(
        json_near_token_yocto(&v_unlock["pending_to_unstake"]).unwrap_or(0) > 0,
        "unlock should queue validator pending_to_unstake"
    );

    // Step 3 — two fresh epochs: pool unstake, then pool withdraw into pending_to_claim.
    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 2).await?;

    // Step 4 — user withdraw receives NEAR.
    let balance_before = buyer.view_account().await?.balance;
    buyer_withdraw(&buyer, staking.id(), pool.id()).await?;
    let balance_after = buyer.view_account().await?.balance;
    assert!(
        balance_after > balance_before,
        "withdraw should transfer claimable NEAR to the buyer"
    );

    let v_done = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v_done["tx_status"]), Some("Idle"));

    Ok(())
}

/// End-to-end farm path:
/// `stake` -> settle stake to pool -> accrue reward in view -> `unstake` ->
/// two settlement passes -> shared `withdraw(validator_id)`.
#[tokio::test]
async fn golden_path_farm_stake_unstake_withdraw_pays_buyer()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, validator_owner, _product_id, _price_id) =
        setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;
    let (farm_product_id, farm_price_id) = create_farm_product_and_price(
        &staking,
        &pool,
        &validator_owner,
        FARM_REWARD_RATE,
        ONE_NEAR_YOCTO,
        None,
    )
    .await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let first_position = buyer_stake_farm(
        &buyer,
        staking.id(),
        &farm_product_id,
        Some(&farm_price_id),
        10,
    )
    .await?;
    assert_eq!(first_position["status"], json!("Active"));

    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 1).await?;
    let v_staked = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_staked["pending_to_stake"]).unwrap_or(0),
        0,
        "epoch_settle should move farm stake onto the pool"
    );

    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    let farm_account: serde_json::Value = worker
        .view(staking.id(), "get_farm_account")
        .args_json(json!({ "account_id": buyer.id() }))
        .await?
        .json()?;
    assert!(
        farm_account["pending_reward_units"]
            .as_str()
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(0)
            > 0,
        "farm account view should simulate unclaimed rewards"
    );
    let account_position = &farm_account["active_positions"][0];
    assert!(
        json_u128_string(account_position, "staked_near_amount") > 0,
        "farm account active position should include staked NEAR"
    );
    assert!(
        json_u128_string(account_position, "pending_reward_units") > 0,
        "farm account active position should include pending rewards"
    );
    assert_eq!(
        json_u128_string(account_position, "total_earned_reward_units"),
        json_u128_string(account_position, "pending_reward_units")
    );

    buyer_unstake_farm(&buyer, staking.id(), &farm_product_id, None).await?;
    let closed_position: serde_json::Value = worker
        .view(staking.id(), "get_farm_position")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": farm_product_id,
        }))
        .await?
        .json()?;
    assert_eq!(closed_position["status"], json!("Closed"));
    assert_eq!(closed_position["shares"], json!("0"));
    assert_eq!(json_u128_string(&closed_position, "staked_near_amount"), 0);
    assert_eq!(
        json_u128_string(&closed_position, "pending_reward_units"),
        0
    );
    assert_eq!(
        json_u128_string(&closed_position, "total_earned_reward_units"),
        0
    );

    let rolled_account: serde_json::Value = worker
        .view(staking.id(), "get_farm_account")
        .args_json(json!({ "account_id": buyer.id() }))
        .await?
        .json()?;
    assert!(
        rolled_account["accumulated_reward_units"]
            .as_str()
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(0)
            > 0,
        "full unstake should roll rewards into the farm account"
    );

    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 2).await?;
    let balance_before = buyer.view_account().await?.balance;
    buyer_withdraw(&buyer, staking.id(), pool.id()).await?;
    let balance_after = buyer.view_account().await?.balance;
    assert!(
        balance_after > balance_before,
        "farm withdraw should transfer claimable NEAR to the buyer"
    );
    assert!(
        balance_after.as_yoctonear()
            > balance_before
                .as_yoctonear()
                .saturating_add(NearToken::from_near(9).as_yoctonear()),
        "farm withdraw should return most of the 10 NEAR stake after gas"
    );

    Ok(())
}

/// Partial farm unstake keeps the position active while still routing the exited
/// stake through the shared settlement and withdraw flow.
#[tokio::test]
async fn golden_path_farm_partial_unstake_keeps_position_active()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, validator_owner, _product_id, _price_id) =
        setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;
    let (farm_product_id, farm_price_id) = create_farm_product_and_price(
        &staking,
        &pool,
        &validator_owner,
        FARM_REWARD_RATE,
        ONE_NEAR_YOCTO,
        None,
    )
    .await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let first_position = buyer_stake_farm(
        &buyer,
        staking.id(),
        &farm_product_id,
        Some(&farm_price_id),
        10,
    )
    .await?;
    let initial_shares = json_u128_string(&first_position, "shares");
    assert!(initial_shares > 0);

    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 1).await?;
    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;

    buyer_unstake_farm(
        &buyer,
        staking.id(),
        &farm_product_id,
        Some(FOUR_NEAR_YOCTO),
    )
    .await?;
    let partial_position: serde_json::Value = worker
        .view(staking.id(), "get_farm_position")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": farm_product_id,
        }))
        .await?
        .json()?;
    let remaining_shares = json_u128_string(&partial_position, "shares");
    assert_eq!(partial_position["status"], json!("Active"));
    assert!(remaining_shares > 0);
    assert!(remaining_shares < initial_shares);
    assert!(
        json_u128_string(&partial_position, "accrued_reward_units") > 0,
        "partial unstake should keep settled rewards on the active position"
    );
    assert!(json_u128_string(&partial_position, "staked_near_amount") > 0);
    assert!(
        json_u128_string(&partial_position, "pending_reward_units")
            >= json_u128_string(&partial_position, "accrued_reward_units")
    );
    assert_eq!(
        json_u128_string(&partial_position, "total_earned_reward_units"),
        json_u128_string(&partial_position, "pending_reward_units")
    );

    let farm_account: serde_json::Value = worker
        .view(staking.id(), "get_farm_account")
        .args_json(json!({ "account_id": buyer.id() }))
        .await?
        .json()?;
    assert_eq!(
        json_u128_string(&farm_account, "accumulated_reward_units"),
        0
    );
    assert!(
        json_u128_string(&farm_account, "pending_reward_units") > 0,
        "remaining active stake should still have unclaimed rewards"
    );

    drive_validator_settlement_epochs(&worker, &operator, staking.id(), pool.id(), 2).await?;
    let balance_before = buyer.view_account().await?.balance;
    buyer_withdraw(&buyer, staking.id(), pool.id()).await?;
    let balance_after = buyer.view_account().await?.balance;
    assert!(
        balance_after > balance_before,
        "partial farm withdraw should transfer claimable NEAR to the buyer"
    );

    Ok(())
}
