//! Sandbox E2E tests for the **epoch settlement pipeline** ([`epoch.rs`](../src/epoch.rs)).
//!
//! Exercises real cross-contract promises against [`mock-staking-pool-contract`]: `get_account`,
//! `deposit_and_stake`, `unstake`, withdraw-from-pool, and public `epoch_settle` retries.
//!
//! Build WASMs from repo root: `make staking-contract`, `make mock-staking-pool-contract`.
//! Run: `cargo test -p staking-contract --test sandbox_epoch_settlement`

mod mock_pool;

use mock_pool::{
    SETTLEMENT_PIPELINE_GAS_TGAS, buyer_cancel_subscription, buyer_lock_one_off,
    buyer_lock_subscription, buyer_storage_deposit, buyer_unlock, buyer_withdraw,
    buyer_withdraw_result, call_epoch_settle, create_subscription_product_and_price,
    eprintln_wait_window_stage, fast_forward_blocks_chunked, fast_forward_until_epoch_delta,
    fast_forward_until_timestamp, fetch_validator, json_near_token_yocto, json_tx_status,
    json_u64_field, json_u64_field_any, pool_set_fail_get_account, pool_set_fail_next_withdraw,
    pool_total_balance_yocto, set_mock_timestamp, setup_staking_fixture,
    setup_staking_fixture_with_unstake_settle_epochs, top_up_buyer_near,
};
use serde_json::json;
use std::time::Instant;

const SHORT_LOCK_NS: &str = "1000000000";

#[tokio::test]
async fn lock_runs_settlement_pipeline_and_clears_tx_status()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    buyer_lock_one_off(&buyer, staking.id(), &price_id, "1000000000000000", 50).await?;

    let v = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v["tx_status"]),
        Some("Idle"),
        "lock promise chain must release validator pipeline mutex"
    );
    assert!(
        json_near_token_yocto(&v["pending_to_stake"]).unwrap_or(0) > 0,
        "lock mints shares and queues pending_to_stake; pool stake follows a later settle"
    );
    let pool_total = pool_total_balance_yocto(&worker, pool.id(), staking.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v["total_staked_balance"]).unwrap_or(0),
        pool_total,
        "pre-user refresh should keep cached total_staked_balance aligned with the pool view"
    );

    Ok(())
}

#[tokio::test]
async fn lock_after_fresh_epoch_pool_stake_keeps_dispatch_release_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let first_lock_id =
        buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let v_after_first = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert!(
        json_near_token_yocto(&v_after_first["pending_to_stake"]).unwrap_or(0) > 0,
        "first lock should leave stake pending for the next fresh-epoch settlement"
    );

    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    top_up_buyer_near(&worker, &buyer, 75).await?;
    let second_lock_id =
        buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    assert_ne!(
        first_lock_id, second_lock_id,
        "fresh-epoch lock should return the new value after settlement dispatch"
    );

    let v_after_second = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v_after_second["tx_status"]),
        Some("Idle"),
        "fresh-epoch pool stake plus value-returning dispatch must release Busy"
    );
    assert!(
        pool_total_balance_yocto(&worker, pool.id(), staking.id()).await? > 0,
        "second lock should first settle the first lock's pending stake onto the pool"
    );
    assert!(
        json_near_token_yocto(&v_after_second["pending_to_stake"]).unwrap_or(0) > 0,
        "second lock should queue its own pending stake after dispatch"
    );

    Ok(())
}

#[tokio::test]
async fn epoch_settle_fast_path_succeeds_when_slot_already_consumed()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    buyer_lock_one_off(&buyer, staking.id(), &price_id, "1000000000000000", 50).await?;

    let again = call_epoch_settle(&buyer, staking.id(), pool.id()).await?;
    again.into_result()?;

    let v = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v["tx_status"]), Some("Idle"));

    Ok(())
}

#[tokio::test]
async fn epoch_settle_noop_fresh_epoch_advances_last_settlement_epoch()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, _price_id) = setup_staking_fixture(&worker).await?;
    let caller = worker.dev_create_account().await?;

    fast_forward_until_epoch_delta(&worker, 1, Some(&caller), Some(staking.id())).await?;
    let current_epoch: u64 = caller
        .view(staking.id(), "get_epoch_height")
        .await?
        .json()?;

    let v_before = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_before["pending_to_stake"]).unwrap_or(0),
        0,
        "fixture should start without pending stake"
    );
    assert_eq!(
        json_near_token_yocto(&v_before["pending_to_unstake"]).unwrap_or(0),
        0,
        "fixture should start without pending unstake"
    );
    assert!(
        json_u64_field_any(&v_before["last_settlement_epoch"]).unwrap_or(0) < current_epoch,
        "test must exercise a fresh-epoch settlement"
    );

    call_epoch_settle(&caller, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v_after["tx_status"]), Some("Idle"));
    assert_eq!(
        json_u64_field_any(&v_after["last_settlement_epoch"]),
        Some(current_epoch),
        "successful no-op epoch_settle should mark the current epoch handled"
    );
    assert_eq!(
        json_near_token_yocto(&v_after["pending_to_stake"]).unwrap_or(0),
        0
    );
    assert_eq!(
        json_near_token_yocto(&v_after["pending_to_unstake"]).unwrap_or(0),
        0
    );

    Ok(())
}

#[tokio::test]
async fn lock_no_pending_fresh_epoch_marks_settlement_handled()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    let current_epoch: u64 = buyer.view(staking.id(), "get_epoch_height").await?.json()?;

    let v_before = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_before["pending_to_stake"]).unwrap_or(0),
        0,
        "fixture should start without pending stake"
    );
    assert_eq!(
        json_near_token_yocto(&v_before["pending_to_unstake"]).unwrap_or(0),
        0,
        "fixture should start without pending unstake"
    );
    assert!(
        json_u64_field_any(&v_before["last_settlement_epoch"]).unwrap_or(0) < current_epoch,
        "test must exercise a fresh-epoch pre-action settlement"
    );

    buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let v_after = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v_after["tx_status"]), Some("Idle"));
    assert_eq!(
        json_u64_field_any(&v_after["last_settlement_epoch"]),
        Some(current_epoch),
        "successful no-pending pre-action settlement should mark the current epoch handled"
    );
    assert!(
        json_near_token_yocto(&v_after["pending_to_stake"]).unwrap_or(0) > 0,
        "the lock tail should still queue its own stake for a later epoch"
    );

    Ok(())
}

#[tokio::test]
async fn epoch_settle_get_account_failure_releases_busy_and_allows_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, owner, _product_id, _price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    pool_set_fail_get_account(&owner, pool.id(), true).await?;

    // `get_account` callback failure is handled gracefully: top-level tx may succeed or fail
    // depending on receipt routing, but pipeline `Busy` must always be released.
    let _first_attempt = call_epoch_settle(&buyer, staking.id(), pool.id()).await?;

    let v_after_fail = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v_after_fail["tx_status"]),
        Some("Idle"),
        "failed get_account callback must still release pipeline Busy"
    );

    pool_set_fail_get_account(&owner, pool.id(), false).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after_retry = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v_after_retry["tx_status"]), Some("Idle"));

    Ok(())
}

#[tokio::test]
async fn lock_refunds_deposit_when_get_account_fails() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let balance_before = buyer.view_account().await?.balance;

    pool_set_fail_get_account(&owner, pool.id(), true).await?;

    let lock_outcome = buyer
        .call(staking.id(), "lock")
        .args_json(json!({
            "price_id": price_id,
            "duration_ns": SHORT_LOCK_NS,
            "product_id": null,
        }))
        .deposit(near_workspaces::types::NearToken::from_near(50))
        .gas(near_workspaces::types::Gas::from_tgas(
            SETTLEMENT_PIPELINE_GAS_TGAS,
        ))
        .transact()
        .await?;

    pool_set_fail_get_account(&owner, pool.id(), false).await?;

    let balance_after = buyer.view_account().await?.balance;
    assert!(
        balance_after
            >= balance_before.saturating_sub(near_workspaces::types::NearToken::from_millinear(50)),
        "lock deposit must be refunded after get_account failure (balance_before={balance_before:?} balance_after={balance_after:?})"
    );

    let v = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v["tx_status"]),
        Some("Idle"),
        "payable abort must release pipeline Busy"
    );

    let pending_stake = json_near_token_yocto(&v["pending_to_stake"]).unwrap_or(0);
    assert_eq!(
        pending_stake, 0,
        "failed lock must not queue pending_to_stake"
    );

    let _ = lock_outcome;

    Ok(())
}

#[tokio::test]
async fn epoch_settle_same_epoch_fast_path_leaves_pending_until_next_epoch()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer_a = worker.dev_create_account().await?;
    let buyer_b = worker.dev_create_account().await?;

    for buyer in [&buyer_a, &buyer_b] {
        buyer_storage_deposit(buyer, staking.id()).await?;
        buyer_lock_one_off(buyer, staking.id(), &price_id, "1000000000000000", 50).await?;
    }

    let v_mid = fetch_validator(&worker, staking.id(), pool.id()).await?;
    let pending_mid = json_near_token_yocto(&v_mid["pending_to_stake"]).unwrap_or(0);
    assert!(
        pending_mid > 0,
        "second lock in the same NEAR epoch should leave stake pending until a later epoch"
    );

    // Fast-path `epoch_settle` (slot already used this NEAR epoch) is a no-op for pending queues.
    call_epoch_settle(&buyer_a, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_after["pending_to_stake"]).unwrap_or(0),
        pending_mid,
        "same-epoch epoch_settle must not clear pending_to_stake; advance epoch and settle again"
    );

    Ok(())
}

#[tokio::test]
async fn two_locks_then_epoch_settle_next_epoch_stakes_on_pool()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer_a = worker.dev_create_account().await?;
    let buyer_b = worker.dev_create_account().await?;

    for buyer in [&buyer_a, &buyer_b] {
        buyer_storage_deposit(buyer, staking.id()).await?;
        buyer_lock_one_off(buyer, staking.id(), &price_id, "1000000000000000", 50).await?;
    }

    let pool_before = pool_total_balance_yocto(&worker, pool.id(), staking.id()).await?;

    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer_a), Some(staking.id())).await?;
    call_epoch_settle(&buyer_a, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v["pending_to_stake"]).unwrap_or(0),
        0,
        "epoch_settle in a fresh NEAR epoch should clear accumulated pending_to_stake"
    );

    let pool_after = pool_total_balance_yocto(&worker, pool.id(), staking.id()).await?;
    assert!(
        pool_after > pool_before,
        "pool should receive combined stake from epoch_settle"
    );

    Ok(())
}

#[tokio::test]
async fn unlock_queues_unstake_then_epoch_settle_next_epoch_clears_pending()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let lock: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        &worker,
        end_ns.saturating_add(1),
        Some(&buyer),
        Some(staking.id()),
    )
    .await?;
    buyer_unlock(&buyer, staking.id(), &lock_id).await?;

    let v_after_unlock = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert!(
        json_near_token_yocto(&v_after_unlock["pending_to_unstake"]).unwrap_or(0) > 0,
        "unlock should queue NEAR for the next pool unstake"
    );
    assert_eq!(
        json_tx_status(&v_after_unlock["tx_status"]),
        Some("Idle"),
        "unlock pipeline must release Busy after share exit"
    );

    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_settled = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_settled["pending_to_unstake"]).unwrap_or(0),
        0,
        "epoch_settle should run pool unstake and clear pending_to_unstake"
    );

    Ok(())
}

#[tokio::test]
async fn repeated_unstake_wait_window_does_not_wedge_busy() -> Result<(), Box<dyn std::error::Error>>
{
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) =
        setup_staking_fixture_with_unstake_settle_epochs(&worker, 20).await?;
    let buyer_a = worker.dev_create_account().await?;
    let buyer_b = worker.dev_create_account().await?;

    for buyer in [&buyer_a, &buyer_b] {
        buyer_storage_deposit(buyer, staking.id()).await?;
    }
    let lock_a = buyer_lock_one_off(&buyer_a, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;
    let lock_b = buyer_lock_one_off(&buyer_b, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let v_after_locks = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(&worker, staking.id(), "after both locks", &v_after_locks).await?;

    // Stake both locks first.
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] before first ff epoch_id={:?}",
        b.epoch_id()
    );
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer_a), Some(staking.id())).await?;
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] after first ff epoch_id={:?}",
        b.epoch_id()
    );
    call_epoch_settle(&buyer_a, staking.id(), pool.id())
        .await?
        .into_result()?;
    let v_after_first_settle = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(
        &worker,
        staking.id(),
        "after first settle",
        &v_after_first_settle,
    )
    .await?;

    // Both locks become unlockable.
    for lock_id in [&lock_a, &lock_b] {
        let lock: serde_json::Value = worker
            .view(staking.id(), "get_lock")
            .args_json(json!({ "lock_id": lock_id }))
            .await?
            .json()?;
        let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
        fast_forward_until_timestamp(
            &worker,
            end_ns.saturating_add(1),
            Some(&buyer_a),
            Some(staking.id()),
        )
        .await?;
    }

    // First unlock + settle performs one pool unstake and records last_unstake_epoch.
    buyer_unlock(&buyer_a, staking.id(), &lock_a).await?;
    let v_after_unlock_a = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(&worker, staking.id(), "after unlock A", &v_after_unlock_a).await?;
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] before second ff epoch_id={:?}",
        b.epoch_id()
    );
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer_a), Some(staking.id())).await?;
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] after second ff epoch_id={:?}",
        b.epoch_id()
    );
    call_epoch_settle(&buyer_a, staking.id(), pool.id())
        .await?
        .into_result()?;
    let v_after_second_settle = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(
        &worker,
        staking.id(),
        "after second settle",
        &v_after_second_settle,
    )
    .await?;

    // Queue another unstake while unstake-wait window is still active.
    buyer_unlock(&buyer_b, staking.id(), &lock_b).await?;
    let v_after_unlock_b = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(&worker, staking.id(), "after unlock B", &v_after_unlock_b).await?;

    // Fresh epoch but still inside unstake wait window: should skip unstake safely and release Busy.
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] before third ff epoch_id={:?}",
        b.epoch_id()
    );
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer_a), Some(staking.id())).await?;
    let b = worker.view_block().await?;
    eprintln!(
        "[timing][wait-window] after third ff epoch_id={:?}",
        b.epoch_id()
    );
    let current_epoch: u64 = buyer_b
        .view(staking.id(), "get_epoch_height")
        .await?
        .json()?;
    call_epoch_settle(&buyer_b, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v = fetch_validator(&worker, staking.id(), pool.id()).await?;
    eprintln_wait_window_stage(&worker, staking.id(), "final", &v).await?;
    assert_eq!(
        json_tx_status(&v["tx_status"]),
        Some("Idle"),
        "too-early repeated unstake settle must not wedge validator in Busy"
    );
    assert!(
        json_near_token_yocto(&v["pending_to_unstake"]).unwrap_or(0) > 0,
        "pending_to_unstake should remain queued until unstake wait window finishes"
    );
    assert_eq!(
        json_u64_field_any(&v["last_settlement_epoch"]),
        Some(current_epoch),
        "unstake-waiting settlement should mark the current epoch handled"
    );

    Ok(())
}

#[tokio::test]
async fn epoch_settle_net_zero_when_stake_and_unstake_pending_match()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    // Stake the first lock's pending queue in a fresh epoch.
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let lock: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        &worker,
        end_ns.saturating_add(1),
        Some(&buyer),
        Some(staking.id()),
    )
    .await?;

    // Same NEAR epoch: unlock queues unstake; a second lock queues matching stake (no pool op yet).
    top_up_buyer_near(&worker, &buyer, 50).await?;
    buyer_unlock(&buyer, staking.id(), &lock_id).await?;
    buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let v_before = fetch_validator(&worker, staking.id(), pool.id()).await?;
    let stake_p = json_near_token_yocto(&v_before["pending_to_stake"]).unwrap_or(0);
    let unstake_p = json_near_token_yocto(&v_before["pending_to_unstake"]).unwrap_or(0);
    assert!(
        stake_p > 0 && unstake_p > 0,
        "unlock + lock in the same epoch should leave both pending queues non-zero"
    );
    assert_eq!(
        stake_p, unstake_p,
        "matching 50 NEAR lock/unlock should produce equal pending stake and unstake"
    );

    let pool_before = pool_total_balance_yocto(&worker, pool.id(), staking.id()).await?;

    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_after["pending_to_stake"]).unwrap_or(0),
        0
    );
    assert_eq!(
        json_near_token_yocto(&v_after["pending_to_unstake"]).unwrap_or(0),
        0
    );
    let pool_after = pool_total_balance_yocto(&worker, pool.id(), staking.id()).await?;
    assert_eq!(
        pool_after, pool_before,
        "net-zero settle should clear pending without a pool deposit_and_stake or unstake"
    );

    Ok(())
}

#[tokio::test]
async fn withdraw_runs_settlement_prefetch_before_payout() -> Result<(), Box<dyn std::error::Error>>
{
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let lock: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        &worker,
        end_ns.saturating_add(1),
        Some(&buyer),
        Some(staking.id()),
    )
    .await?;

    buyer_unlock(&buyer, staking.id(), &lock_id).await?;

    // Unlock only queues pending_to_unstake. First fresh epoch settle performs pool `unstake`.
    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;
    // Second fresh epoch settle pulls unstaked NEAR from pool into pending_to_withdraw.
    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;

    let balance_before = buyer.view_account().await?.balance;
    buyer_withdraw(&buyer, staking.id(), pool.id()).await?;

    let balance_after = buyer.view_account().await?.balance;
    assert!(
        balance_after > balance_before,
        "withdraw should transfer NEAR after settlement prefetch and tranche payout"
    );

    Ok(())
}

#[tokio::test]
async fn failed_pool_withdraw_skips_settle_and_does_not_mark_epoch_handled()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;

    let lock: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        &worker,
        end_ns.saturating_add(1),
        Some(&buyer),
        Some(staking.id()),
    )
    .await?;
    buyer_unlock(&buyer, staking.id(), &lock_id).await?;

    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after_unstake = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert!(
        json_near_token_yocto(&v_after_unstake["pending_to_withdraw"]).unwrap_or(0) > 0,
        "pool unstake should queue pending_to_withdraw before the withdraw retry scenario"
    );

    top_up_buyer_near(&worker, &buyer, 50).await?;
    buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;
    let v_after_second_lock = fetch_validator(&worker, staking.id(), pool.id()).await?;
    let pending_to_stake_before_fail =
        json_near_token_yocto(&v_after_second_lock["pending_to_stake"]).unwrap_or(0);
    assert!(
        pending_to_stake_before_fail > 0,
        "second lock should queue pending stake before the failed-withdraw scenario"
    );

    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    let current_epoch: u64 = operator
        .view(staking.id(), "get_epoch_height")
        .await?
        .json()?;
    pool_set_fail_next_withdraw(&owner, pool.id(), true).await?;

    let _failed_withdraw_attempt = call_epoch_settle(&operator, staking.id(), pool.id()).await?;

    let v_after_failed_withdraw = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v_after_failed_withdraw["tx_status"]),
        Some("Idle"),
        "failed pool withdraw path must release Busy"
    );
    assert!(
        json_near_token_yocto(&v_after_failed_withdraw["pending_to_withdraw"]).unwrap_or(0) > 0,
        "failed pool withdraw must leave pending_to_withdraw queued"
    );
    assert_eq!(
        json_near_token_yocto(&v_after_failed_withdraw["pending_to_stake"]).unwrap_or(0),
        pending_to_stake_before_fail,
        "failed pool withdraw must skip stake/unstake settlement"
    );
    assert!(
        json_u64_field_any(&v_after_failed_withdraw["last_settlement_epoch"]).unwrap_or(0)
            < current_epoch,
        "failed pool withdraw must not mark the current epoch handled"
    );

    pool_set_fail_next_withdraw(&owner, pool.id(), false).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;

    let v_after_retry = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_near_token_yocto(&v_after_retry["pending_to_withdraw"]).unwrap_or(0),
        0,
        "same-epoch retry should still reach pool withdraw"
    );
    assert!(
        json_near_token_yocto(&v_after_retry["pending_to_claim"]).unwrap_or(0) > 0,
        "successful retry should move pool funds into pending_to_claim"
    );
    assert_eq!(
        json_u64_field_any(&v_after_retry["last_settlement_epoch"]),
        Some(current_epoch),
        "successful same-epoch retry can mark the epoch handled after withdraw succeeds"
    );

    Ok(())
}

#[tokio::test]
async fn early_withdraw_failure_still_releases_busy_and_later_retry_succeeds()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let (staking, pool, _owner, _product_id, price_id) = setup_staking_fixture(&worker).await?;
    let buyer = worker.dev_create_account().await?;
    let operator = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_one_off(&buyer, staking.id(), &price_id, SHORT_LOCK_NS, 50).await?;

    let lock: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        &worker,
        end_ns.saturating_add(1),
        Some(&buyer),
        Some(staking.id()),
    )
    .await?;
    buyer_unlock(&buyer, staking.id(), &lock_id).await?;

    // Too early for tranche claimability; withdraw should fail but must not wedge Busy.
    fast_forward_blocks_chunked(&worker, 50).await?;
    let balance_before_early = buyer.view_account().await?.balance;
    let _early = buyer_withdraw_result(&buyer, staking.id(), pool.id()).await?;
    let balance_after_early = buyer.view_account().await?.balance;
    assert!(
        balance_after_early <= balance_before_early,
        "early withdraw must not increase buyer balance before tranche claimability"
    );

    let v_after_fail = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(
        json_tx_status(&v_after_fail["tx_status"]),
        Some("Idle"),
        "failed withdraw tail must still release pipeline Busy"
    );

    // Drive settlement deterministically: one epoch to run pool unstake, one more to pull unstaked funds.
    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;
    fast_forward_until_epoch_delta(&worker, 1, Some(&operator), Some(staking.id())).await?;
    call_epoch_settle(&operator, staking.id(), pool.id())
        .await?
        .into_result()?;
    let balance_before_retry = buyer.view_account().await?.balance;
    buyer_withdraw(&buyer, staking.id(), pool.id()).await?;
    let balance_after_retry = buyer.view_account().await?.balance;
    assert!(
        balance_after_retry > balance_before_retry,
        "retry withdraw should transfer claimable NEAR after settlement catches up"
    );

    let v_after_retry = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert_eq!(json_tx_status(&v_after_retry["tx_status"]), Some("Idle"));

    Ok(())
}

#[tokio::test]
async fn cancel_subscription_after_long_idle_normalizes_end_and_renews_from_fresh_subscription()
-> Result<(), Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    eprintln!(
        "[timing] start cancel_subscription_after_long_idle_normalizes_end_and_renews_from_fresh_subscription"
    );
    let worker = near_workspaces::sandbox().await?;
    eprintln!("[timing] sandbox ready: {:?}", t0.elapsed());
    // Use test-feature WASM for mock clock controls (instant time advancement)
    let (staking, pool, owner, _product_id_oneoff, _price_id_oneoff) =
        setup_staking_fixture(&worker).await?;
    let (sub_product_id, sub_price_id) =
        create_subscription_product_and_price(&staking, &pool, &owner).await?;
    eprintln!("[timing] fixture+catalog ready: {:?}", t0.elapsed());
    let buyer = worker.dev_create_account().await?;

    let t_step = Instant::now();
    buyer_storage_deposit(&buyer, staking.id()).await?;
    eprintln!("[timing] buyer_storage_deposit: {:?}", t_step.elapsed());
    let t_step = Instant::now();
    let _first_lock = buyer_lock_subscription(&buyer, staking.id(), &sub_price_id, 50).await?;
    eprintln!(
        "[timing] first buyer_lock_subscription: {:?}",
        t_step.elapsed()
    );

    let t_step = Instant::now();
    let sub_initial: serde_json::Value = worker
        .view(staking.id(), "get_subscription_for_product")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": sub_product_id,
        }))
        .await?
        .json()?;
    eprintln!(
        "[timing] first get_subscription_for_product view: {:?}",
        t_step.elapsed()
    );
    let sid_initial = sub_initial["subscription_id"]
        .as_str()
        .expect("subscription_id")
        .to_string();
    let start_ns = json_u64_field(&sub_initial["start_ns"]).expect("start_ns");
    let end_ns = json_u64_field(&sub_initial["end_ns"]).expect("end_ns");
    let period_ns = end_ns.saturating_sub(start_ns);
    let late_ts = end_ns
        .saturating_add(period_ns.saturating_mul(2))
        .saturating_add(1);
    let t_step = Instant::now();
    set_mock_timestamp(&buyer, staking.id(), late_ts).await?;
    eprintln!("[timing] first mock timestamp set: {:?}", t_step.elapsed());

    buyer_cancel_subscription(&buyer, staking.id(), &sid_initial).await?;

    let sub_after_cancel: serde_json::Value = worker
        .view(staking.id(), "get_subscription_for_product")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": sub_product_id,
        }))
        .await?
        .json()?;
    let cancel_end_ns = json_u64_field(&sub_after_cancel["end_ns"]).expect("end_ns");
    assert!(
        cancel_end_ns > late_ts,
        "cancel-at-period-end should normalize stale subscription window to current virtual cycle"
    );
    assert_eq!(sub_after_cancel["cancel_at_period_end"], json!(true));

    let t_step = Instant::now();
    set_mock_timestamp(&buyer, staking.id(), cancel_end_ns.saturating_add(1)).await?;
    eprintln!("[timing] second mock timestamp set: {:?}", t_step.elapsed());

    // Renewal needs another 50 NEAR attached; dev account is ~50 NEAR after the first lock + gas.
    top_up_buyer_near(&worker, &buyer, 50).await?;
    let _second_lock = buyer_lock_subscription(&buyer, staking.id(), &sub_price_id, 50).await?;

    let sub_after_renew: serde_json::Value = worker
        .view(staking.id(), "get_subscription_for_product")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": sub_product_id,
        }))
        .await?
        .json()?;
    let sid_after = sub_after_renew["subscription_id"]
        .as_str()
        .expect("subscription_id");
    assert_ne!(
        sid_after, sid_initial,
        "after cancel-at-period-end boundary, renewal should create a fresh subscription row"
    );
    assert_eq!(sub_after_renew["cancel_at_period_end"], json!(false));
    eprintln!("[timing] test total: {:?}", t0.elapsed());

    Ok(())
}

#[tokio::test]
async fn cancel_subscription_after_long_idle_then_unlock_requests_unstake()
-> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    // Use test-feature WASM for mock clock controls (instant time advancement)
    let (staking, pool, owner, _product_id_oneoff, _price_id_oneoff) =
        setup_staking_fixture(&worker).await?;
    let (sub_product_id, sub_price_id) =
        create_subscription_product_and_price(&staking, &pool, &owner).await?;
    let buyer = worker.dev_create_account().await?;

    buyer_storage_deposit(&buyer, staking.id()).await?;
    let lock_id = buyer_lock_subscription(&buyer, staking.id(), &sub_price_id, 50).await?;

    // Ensure initial pending stake is settled before we run unlock later.
    fast_forward_until_epoch_delta(&worker, 1, Some(&buyer), Some(staking.id())).await?;
    call_epoch_settle(&buyer, staking.id(), pool.id())
        .await?
        .into_result()?;

    let sub_initial: serde_json::Value = worker
        .view(staking.id(), "get_subscription_for_product")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": sub_product_id,
        }))
        .await?
        .json()?;
    let sid_initial = sub_initial["subscription_id"]
        .as_str()
        .expect("subscription_id")
        .to_string();
    let start_ns = json_u64_field(&sub_initial["start_ns"]).expect("start_ns");
    let end_ns = json_u64_field(&sub_initial["end_ns"]).expect("end_ns");
    let period_ns = end_ns.saturating_sub(start_ns);
    let late_ts = end_ns
        .saturating_add(period_ns.saturating_mul(2))
        .saturating_add(1);
    set_mock_timestamp(&buyer, staking.id(), late_ts).await?;

    buyer_cancel_subscription(&buyer, staking.id(), &sid_initial).await?;
    let sub_after_cancel: serde_json::Value = worker
        .view(staking.id(), "get_subscription_for_product")
        .args_json(json!({
            "account_id": buyer.id(),
            "product_id": sub_product_id,
        }))
        .await?
        .json()?;
    let cancel_end_ns = json_u64_field(&sub_after_cancel["end_ns"]).expect("end_ns");
    set_mock_timestamp(&buyer, staking.id(), cancel_end_ns.saturating_add(1)).await?;

    buyer_unlock(&buyer, staking.id(), &lock_id).await?;

    let lock_after: serde_json::Value = worker
        .view(staking.id(), "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    assert_eq!(lock_after["status"], json!("UnlockRequested"));

    let v_after_unlock = fetch_validator(&worker, staking.id(), pool.id()).await?;
    assert!(
        json_near_token_yocto(&v_after_unlock["pending_to_unstake"]).unwrap_or(0) > 0,
        "unlock after cancel-at-end should queue pending_to_unstake for later settle"
    );
    assert_eq!(
        json_tx_status(&v_after_unlock["tx_status"]),
        Some("Idle"),
        "unlock flow should release validator Busy status"
    );

    Ok(())
}
