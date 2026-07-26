//! Sandbox helpers: deploy [`mock-staking-pool-contract`] + [`staking-contract`] WASM and drive flows.
//! Build from repo root: `make staking-contract`, `make mock-staking-pool-contract`.

use near_workspaces::Worker;
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::types::{Gas as WsGas, NearToken};
use serde_json::json;
use std::path::Path;

/// `10^24` — matches `staking_contract::utils::LOCK_FACTOR_DENOM`.
pub const LOCK_FACTOR_DENOM: &str = "1000000000000000000000000";

/// Prepaid gas for `lock`, `unlock`, `withdraw`, and `epoch_settle`.
/// NEAR caps prepaid gas at 300 TGas per transaction; long settlement promise chains
/// forward unused gas from this budget.
pub const SETTLEMENT_PIPELINE_GAS_TGAS: u64 = 300;

/// Resolve WASM bytes from typical build outputs (`make …`, `cargo near`, or `cargo build --target wasm32`).
pub fn wasm_from_candidates(rel_paths: &[&str]) -> Result<Vec<u8>, std::io::Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for rel in rel_paths {
        let p = root.join(rel);
        if let Ok(bytes) = std::fs::read(&p) {
            return Ok(bytes);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("wasm not found (tried {rel_paths:?})"),
    ))
}

pub fn staking_wasm_bytes() -> Result<Vec<u8>, std::io::Error> {
    wasm_from_candidates(&[
        "res/local/staking_contract.wasm",
        "target/near/staking_contract/staking_contract.wasm",
        "target/wasm32-unknown-unknown/release/staking_contract.wasm",
    ])
}

/// Load test-feature WASM with mocked clock controls (`set_block_timestamp`, `set_epoch_height`).
pub fn staking_wasm_bytes_test() -> Result<Vec<u8>, std::io::Error> {
    wasm_from_candidates(&[
        "res/local/staking_contract_test.wasm",
        "target/near/staking_contract/staking_contract.wasm",
        "target/wasm32-unknown-unknown/release/staking_contract.wasm",
    ])
}

pub fn mock_pool_wasm_bytes() -> Result<Vec<u8>, std::io::Error> {
    wasm_from_candidates(&[
        "res/local/mock_staking_pool_contract.wasm",
        "target/near/mock_staking_pool_contract/mock_staking_pool_contract.wasm",
        "target/wasm32-unknown-unknown/release/mock_staking_pool_contract.wasm",
    ])
}

/// Args for `staking-contract` `new` in sandbox (no public `epoch_*`; pool work follows user actions).
pub fn staking_new_args_e2e(owner: &near_workspaces::AccountId) -> serde_json::Value {
    json!({
        "config": {
            "owner_account_id": owner,
            "proposed_new_owner_account_id": null,
            "guardians": [],
            "min_lock_duration_ns": "1",
            "max_lock_duration_ns": "10000000000000000000",
            "epoch_unstake_settle_epochs": 1,
            "min_storage_deposit": "10000000000000000000000",
            "per_lock_storage_stake": "0",
            "per_farm_position_storage_stake": "0",
            "per_purchase_storage_stake": "0",
            "min_lock_amount": "1000000000000000000000000",
        }
    })
}

/// Advance sandbox blocks until the chain timestamp reaches `target_ns` (used for `unlock` after `lock.end_ns`).
///
/// Uses adaptive bounded steps so very large jumps do not stall on one long `fast_forward` call.
///
/// If `staking_id` and `caller` are provided, attempts to use mock timestamp first (instant),
/// falling back to slow fast-forward if mock is unavailable.
pub async fn fast_forward_until_timestamp(
    worker: &Worker<Sandbox>,
    target_ns: u64,
    caller: Option<&near_workspaces::Account>,
    staking_id: Option<&near_workspaces::AccountId>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Try mock method first if staking_id and caller are provided
    if let (Some(staking), Some(caller_acc)) = (staking_id, caller) {
        match set_mock_timestamp(caller_acc, staking, target_ns).await {
            Ok(()) => {
                eprintln!("[timing] set timestamp via mock to {}", target_ns);
                return Ok(());
            }
            Err(_) => {
                eprintln!("[timing] mock unavailable, falling back to fast-forward");
            }
        }
    }

    // Conservative defaults for sandbox stability; tune by observed timestamp deltas.
    const DEFAULT_NS_PER_BLOCK: u64 = 1_000_000_000;
    const MIN_STEP_BLOCKS: u64 = 50;
    const MAX_STEP_BLOCKS: u64 = 20_000;
    const MAX_ROUNDS: u32 = 5_000;
    const MAX_STALLED_ROUNDS: u32 = 12;

    let mut ns_per_block = DEFAULT_NS_PER_BLOCK;
    let mut stalled_rounds = 0u32;
    for round in 0..MAX_ROUNDS {
        let ts_before = worker.view_block().await?.timestamp();
        if ts_before >= target_ns {
            return Ok(());
        }
        let gap = target_ns.saturating_sub(ts_before);
        let blocks_guess = (gap / ns_per_block).max(1);
        let blocks = blocks_guess.clamp(MIN_STEP_BLOCKS, MAX_STEP_BLOCKS);
        if round % 10 == 0 {
            eprintln!(
                "[timing][ff-ts] round={} gap_ns={} step_blocks={} est_ns_per_block={}",
                round, gap, blocks, ns_per_block
            );
        }
        worker.fast_forward(blocks).await?;
        let ts_after = worker.view_block().await?.timestamp();
        let advanced_ns = ts_after.saturating_sub(ts_before);

        if advanced_ns == 0 {
            stalled_rounds = stalled_rounds.saturating_add(1);
            if stalled_rounds >= MAX_STALLED_ROUNDS {
                return Err(format!(
                    "timestamp did not advance after {MAX_STALLED_ROUNDS} rounds (target={target_ns}, last={ts_after})"
                )
                .into());
            }
            continue;
        }

        stalled_rounds = 0;
        ns_per_block = (advanced_ns / blocks).max(1);
    }
    let last = worker.view_block().await?.timestamp();
    Err(format!("timestamp {target_ns} not reached after fast_forward (last {last:?})",).into())
}

/// Set mocked block timestamp directly using contract's test-feature method.
/// This is much faster than `fast_forward_until_timestamp` for large time jumps.
/// Requires staking contract built with `test` feature (loads `staking_contract_test.wasm`).
pub async fn set_mock_timestamp(
    caller: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    target_ns: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    caller
        .call(staking_id, "set_block_timestamp")
        .args_json(json!({ "timestamp_ns": target_ns }))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Fast-forward one subscription's virtual clock using the contract's test-feature helper.
/// Unlike `set_mock_timestamp`, this changes only the selected subscription's virtual now.
pub async fn fast_forward_subscription_for_product_to(
    caller: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    product_id: &str,
    target_ns: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    caller
        .call(staking_id, "test_fast_forward_subscription_for_product_to")
        .args_json(json!({
            "product_id": product_id,
            "target_timestamp_ns": target_ns.to_string(),
        }))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Set mocked epoch height directly using contract's test-feature method.
/// This is much faster than `fast_forward_until_epoch_delta` for large epoch jumps.
/// Requires staking contract built with `test` feature (loads `staking_contract_test.wasm`).
pub async fn set_mock_epoch(
    caller: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    target_epoch: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    caller
        .call(staking_id, "set_epoch_height")
        .args_json(json!({ "epoch": target_epoch }))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Advance by many blocks in smaller chunks to avoid long single fast-forward calls.
pub async fn fast_forward_blocks_chunked(
    worker: &Worker<Sandbox>,
    total_blocks: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    const STEP: u64 = 10_000;
    let mut left = total_blocks;
    while left > 0 {
        let step = left.min(STEP);
        worker.fast_forward(step).await?;
        left = left.saturating_sub(step);
    }
    Ok(())
}

/// Advance until at least `delta_epochs` epoch-id transitions are observed.
///
/// If `staking_id` and `caller` are provided, attempts to use mock epoch advancement first (instant),
/// falling back to slow fast-forward if mock is unavailable.
pub async fn fast_forward_until_epoch_delta(
    worker: &Worker<Sandbox>,
    delta_epochs: u64,
    caller: Option<&near_workspaces::Account>,
    staking_id: Option<&near_workspaces::AccountId>,
) -> Result<(), Box<dyn std::error::Error>> {
    if delta_epochs == 0 {
        return Ok(());
    }

    // Try mock method first if staking_id and caller are provided
    if let (Some(staking), Some(caller_acc)) = (staking_id, caller) {
        // Get current epoch from contract
        let current_epoch: u64 = caller_acc.view(staking, "get_epoch_height").await?.json()?;
        let target_epoch = current_epoch.saturating_add(delta_epochs);

        match set_mock_epoch(caller_acc, staking, target_epoch).await {
            Ok(()) => {
                eprintln!(
                    "[timing] advanced epoch via mock from {} to {} (delta {})",
                    current_epoch, target_epoch, delta_epochs
                );
                return Ok(());
            }
            Err(_) => {
                eprintln!("[timing] mock unavailable, falling back to fast-forward");
            }
        }
    }

    let mut current_epoch_id = worker.view_block().await?.epoch_id().clone();
    let mut crossed = 0u64;
    const MAX_ROUNDS: u32 = 250;
    for _ in 0..MAX_ROUNDS {
        let now_epoch_id = worker.view_block().await?.epoch_id().clone();
        if now_epoch_id != current_epoch_id {
            crossed = crossed.saturating_add(1);
            current_epoch_id = now_epoch_id;
        }
        if crossed >= delta_epochs {
            return Ok(());
        }
        // Keep each jump moderate to avoid long single fast-forward stalls.
        worker.fast_forward(2_000).await?;
    }
    Err(format!(
        "did not cross {delta_epochs} epoch transitions after fast_forward rounds (crossed {crossed})"
    )
    .into())
}

pub fn near_token_yocto_from_view(
    v: &serde_json::Value,
) -> Result<u128, Box<dyn std::error::Error>> {
    json_near_token_yocto(v).ok_or_else(|| format!("unexpected NearToken JSON: {v}").into())
}

pub fn json_near_token_yocto(v: &serde_json::Value) -> Option<u128> {
    if let Some(s) = v.as_str() {
        return s.parse().ok();
    }
    if let Some(s) = v.get("amount").and_then(|x| x.as_str()) {
        return s.parse().ok();
    }
    if let Some(n) = v.as_u64() {
        return Some(n as u128);
    }
    None
}

pub fn json_u64_field(v: &serde_json::Value) -> Option<u64> {
    v.as_str()?.parse().ok()
}

pub fn json_u64_field_any(v: &serde_json::Value) -> Option<u64> {
    json_u64_field(v).or_else(|| v.as_u64())
}

pub fn json_tx_status(v: &serde_json::Value) -> Option<&str> {
    v.as_str()
}

/// Validator row from `get_validator`.
pub async fn fetch_validator(
    worker: &Worker<Sandbox>,
    staking_id: &near_workspaces::AccountId,
    pool_id: &near_workspaces::AccountId,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(worker
        .view(staking_id, "get_validator")
        .args_json(json!({ "validator_id": pool_id }))
        .await?
        .json()?)
}

/// Debug log for early-withdraw sandbox tests: epoch + validator pending buckets.
pub async fn eprintln_early_withdraw_stage(
    staking_id: &near_workspaces::AccountId,
    stage: &str,
    contract_epoch: u64,
    v: &serde_json::Value,
) {
    eprintln!(
        "[early-withdraw] {stage} contract_epoch={contract_epoch} \
         last_settlement_epoch={} last_unstake_epoch={} tx_status={:?} \
         pending_to_stake={} pending_to_unstake={} pending_to_withdraw={} pending_to_claim={}",
        json_u64_field_any(&v["last_settlement_epoch"]).unwrap_or(0),
        json_u64_field_any(&v["last_unstake_epoch"]).unwrap_or(0),
        json_tx_status(&v["tx_status"]),
        json_near_token_yocto(&v["pending_to_stake"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_unstake"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_withdraw"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_claim"]).unwrap_or(0),
    );
    eprintln!("[early-withdraw] {stage} staking={staking_id} validator_json={v}");
}

/// Debug log for wait-window sandbox tests: staking contract NEAR balance + validator pending buckets.
pub async fn eprintln_wait_window_stage(
    worker: &Worker<Sandbox>,
    staking_id: &near_workspaces::AccountId,
    stage: &str,
    v: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let staking_balance_yocto = worker
        .view_account(staking_id)
        .await?
        .balance
        .as_yoctonear();
    eprintln!(
        "[timing][wait-window] {stage} staking_contract_balance_yocto={staking_balance_yocto} \
         last_unstake_epoch={} pending_to_stake={} pending_to_unstake={} pending_to_withdraw={} pending_to_claim={}",
        json_u64_field_any(&v["last_unstake_epoch"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_stake"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_unstake"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_withdraw"]).unwrap_or(0),
        json_near_token_yocto(&v["pending_to_claim"]).unwrap_or(0),
    );
    Ok(())
}

pub async fn buyer_storage_deposit(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer
        .call(staking_id, "storage_deposit")
        .args_json(json!({}))
        .deposit(NearToken::from_millinear(500))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Transfer NEAR from the sandbox root to `buyer` (dev accounts start with ~50 NEAR).
pub async fn top_up_buyer_near(
    worker: &Worker<Sandbox>,
    buyer: &near_workspaces::Account,
    near: u128,
) -> Result<(), Box<dyn std::error::Error>> {
    let outcome = worker
        .root_account()
        .expect("sandbox root account")
        .transfer_near(buyer.id(), NearToken::from_near(near))
        .await?;
    assert!(outcome.is_success(), "buyer balance top-up must succeed");
    Ok(())
}

pub async fn buyer_lock_one_off(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    price_id: &str,
    duration_ns: &str,
    deposit_near: u128,
) -> Result<String, Box<dyn std::error::Error>> {
    let lock_id: String = buyer
        .call(staking_id, "lock")
        .args_json(json!({
            "price_id": price_id,
            "duration_ns": duration_ns,
            "product_id": null,
        }))
        .deposit(NearToken::from_near(deposit_near))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?
        .json()?;
    Ok(lock_id)
}

pub async fn buyer_lock_subscription(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    price_id: &str,
    deposit_near: u128,
) -> Result<String, Box<dyn std::error::Error>> {
    let lock_id: String = buyer
        .call(staking_id, "lock")
        .args_json(json!({
            "price_id": price_id,
            "product_id": null,
            "duration_ns": null,
        }))
        .deposit(NearToken::from_near(deposit_near))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?
        .json()?;
    Ok(lock_id)
}

pub async fn buyer_stake_farm(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    product_id: &str,
    price_id: Option<&str>,
    deposit_near: u128,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let position: serde_json::Value = buyer
        .call(staking_id, "stake")
        .args_json(json!({
            "product_id": product_id,
            "price_id": price_id,
        }))
        .deposit(NearToken::from_near(deposit_near))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?
        .json()?;
    Ok(position)
}

pub async fn buyer_unstake_farm(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    product_id: &str,
    amount_yocto: Option<u128>,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer
        .call(staking_id, "unstake")
        .args_json(json!({
            "product_id": product_id,
            "amount": amount_yocto.map(|amount| amount.to_string()),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn buyer_cancel_subscription(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    subscription_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer
        .call(staking_id, "cancel_subscription")
        .args_json(json!({ "subscription_id": subscription_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn buyer_unlock(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    lock_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer
        .call(staking_id, "unlock")
        .args_json(json!({ "lock_id": lock_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn buyer_withdraw(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    pool_id: &near_workspaces::AccountId,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer_withdraw_result(buyer, staking_id, pool_id)
        .await?
        .into_result()?;
    Ok(())
}

pub async fn buyer_withdraw_result(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    pool_id: &near_workspaces::AccountId,
) -> Result<near_workspaces::result::ExecutionFinalResult, near_workspaces::error::Error> {
    buyer
        .call(staking_id, "withdraw")
        .args_json(json!({ "validator_id": pool_id }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await
}

pub async fn call_epoch_settle(
    caller: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    pool_id: &near_workspaces::AccountId,
) -> Result<near_workspaces::result::ExecutionFinalResult, near_workspaces::error::Error> {
    caller
        .call(staking_id, "epoch_settle")
        .args_json(json!({ "validator_id": pool_id }))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await
}

pub async fn pool_set_fail_get_account(
    validator_owner: &near_workspaces::Account,
    pool_id: &near_workspaces::AccountId,
    fail: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    validator_owner
        .call(pool_id, "set_fail_get_account")
        .args_json(json!({ "fail": fail }))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn pool_set_fail_next_withdraw(
    validator_owner: &near_workspaces::Account,
    pool_id: &near_workspaces::AccountId,
    fail: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    validator_owner
        .call(pool_id, "set_fail_next_withdraw")
        .args_json(json!({ "fail": fail }))
        .gas(WsGas::from_tgas(50))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

pub async fn pool_total_balance_yocto(
    worker: &Worker<Sandbox>,
    pool_id: &near_workspaces::AccountId,
    staking_id: &near_workspaces::AccountId,
) -> Result<u128, Box<dyn std::error::Error>> {
    let bal_json: serde_json::Value = worker
        .view(pool_id, "get_account_total_balance")
        .args_json(json!({ "account_id": staking_id }))
        .await?
        .json()?;
    near_token_yocto_from_view(&bal_json)
}

/// Deploy staking + mock pool, allowlist pool, and create a one-off catalog price.
/// Uses test-feature WASM with mock time/epoch controls for fast test advancement.
pub async fn setup_staking_fixture(
    worker: &Worker<Sandbox>,
) -> Result<
    (
        near_workspaces::Account,
        near_workspaces::Account,
        near_workspaces::Account,
        String,
        String,
    ),
    Box<dyn std::error::Error>,
> {
    let staking_wasm = staking_wasm_bytes_test().map_err(|e| format!("staking test wasm: {e}"))?;
    let pool_wasm = mock_pool_wasm_bytes().map_err(|e| format!("mock pool wasm: {e}"))?;

    let staking = worker.dev_create_account().await?;
    let pool = worker.dev_create_account().await?;
    let validator_owner = worker.dev_create_account().await?;

    deploy_staking_and_mock_pool(
        &staking,
        &pool,
        validator_owner.id(),
        &staking_wasm,
        &pool_wasm,
    )
    .await?;
    add_validator_pair(&staking, &pool).await?;
    let (product_id, price_id) =
        create_one_off_product_and_price(&staking, &pool, &validator_owner).await?;

    Ok((staking, pool, validator_owner, product_id, price_id))
}

/// Same as [`setup_staking_fixture`] but allows overriding `epoch_unstake_settle_epochs`.
pub async fn setup_staking_fixture_with_unstake_settle_epochs(
    worker: &Worker<Sandbox>,
    epoch_unstake_settle_epochs: u64,
) -> Result<
    (
        near_workspaces::Account,
        near_workspaces::Account,
        near_workspaces::Account,
        String,
        String,
    ),
    Box<dyn std::error::Error>,
> {
    let staking_wasm = staking_wasm_bytes_test().map_err(|e| format!("staking test wasm: {e}"))?;
    let pool_wasm = mock_pool_wasm_bytes().map_err(|e| format!("mock pool wasm: {e}"))?;

    let staking = worker.dev_create_account().await?;
    let pool = worker.dev_create_account().await?;
    let validator_owner = worker.dev_create_account().await?;

    staking
        .batch(staking.id())
        .deploy(&staking_wasm)
        .call(
            Function::new("new")
                .args_json(json!({
                    "config": {
                        "owner_account_id": staking.id(),
                        "proposed_new_owner_account_id": null,
                        "guardians": [],
                        "min_lock_duration_ns": "1",
                        "max_lock_duration_ns": "10000000000000000000",
                        "epoch_unstake_settle_epochs": epoch_unstake_settle_epochs,
                        "min_storage_deposit": "10000000000000000000000",
                        "per_lock_storage_stake": "0",
                        "per_farm_position_storage_stake": "0",
                        "per_purchase_storage_stake": "0",
                        "min_lock_amount": "1000000000000000000000000",
                    }
                }))
                .gas(WsGas::from_tgas(50)),
        )
        .transact()
        .await?
        .into_result()?;

    pool.batch(pool.id())
        .deploy(&pool_wasm)
        .call(
            Function::new("new")
                .args_json(json!({ "owner_id": validator_owner.id() }))
                .gas(WsGas::from_tgas(30)),
        )
        .transact()
        .await?
        .into_result()?;

    add_validator_pair(&staking, &pool).await?;
    let (product_id, price_id) =
        create_one_off_product_and_price(&staking, &pool, &validator_owner).await?;

    Ok((staking, pool, validator_owner, product_id, price_id))
}

/// Deploy staking contract on `staking`, mock pool on `pool`; pool owner = `validator_owner`.
pub async fn deploy_staking_and_mock_pool(
    staking: &near_workspaces::Account,
    pool: &near_workspaces::Account,
    validator_owner: &near_workspaces::AccountId,
    staking_wasm: &[u8],
    pool_wasm: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    staking
        .batch(staking.id())
        .deploy(staking_wasm)
        .call(
            Function::new("new")
                .args_json(staking_new_args_e2e(staking.id()))
                .gas(WsGas::from_tgas(50)),
        )
        .transact()
        .await?
        .into_result()?;

    pool.batch(pool.id())
        .deploy(pool_wasm)
        .call(
            Function::new("new")
                .args_json(json!({ "owner_id": validator_owner }))
                .gas(WsGas::from_tgas(30)),
        )
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

pub async fn add_validator_pair(
    staking: &near_workspaces::Account,
    pool: &near_workspaces::Account,
) -> Result<(), Box<dyn std::error::Error>> {
    staking
        .call(staking.id(), "add_validator")
        .args_json(json!({
            "validator_id": pool.id(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Validator owner: create an active one-off product + price on [`staking-contract`] (pool must be allowlisted).
pub async fn create_one_off_product_and_price(
    staking: &near_workspaces::Account,
    pool: &near_workspaces::Account,
    validator_owner: &near_workspaces::Account,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let cp = validator_owner
        .call(staking.id(), "create_product")
        .args_json(json!({
            "validator_id": pool.id(),
            "name": "Fixture Product",
            "description": "sandbox",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cp.is_success(), "create_product: {:#?}", cp.outcomes());
    let product_id: String = cp.into_result()?.json()?;

    let cpr = validator_owner
        .call(staking.id(), "create_price")
        .args_json(json!({
            "product_id": product_id,
            "name": "One-off",
            "description": "",
            "amount": "1",
            "price_type": "OneOff",
            "billing_period": null,
            "lock_factor_near_months": LOCK_FACTOR_DENOM,
            "metadata": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cpr.is_success(), "create_price: {:#?}", cpr.outcomes());
    let price_id: String = cpr.into_result()?.json()?;
    Ok((product_id, price_id))
}

/// Validator owner: create an active recurring-monthly product + price for subscription flows.
pub async fn create_subscription_product_and_price(
    staking: &near_workspaces::Account,
    pool: &near_workspaces::Account,
    validator_owner: &near_workspaces::Account,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let cp = validator_owner
        .call(staking.id(), "create_product")
        .args_json(json!({
            "validator_id": pool.id(),
            "name": "Fixture Sub Product",
            "description": "sandbox-subscription",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cp.is_success(), "create_product: {:#?}", cp.outcomes());
    let product_id: String = cp.into_result()?.json()?;

    let cpr = validator_owner
        .call(staking.id(), "create_price")
        .args_json(json!({
            "product_id": product_id,
            "name": "Monthly",
            "description": "",
            "amount": "1",
            "price_type": "Recurring",
            "billing_period": "Monthly",
            "lock_factor_near_months": LOCK_FACTOR_DENOM,
            "metadata": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cpr.is_success(), "create_price: {:#?}", cpr.outcomes());
    let price_id: String = cpr.into_result()?.json()?;
    Ok((product_id, price_id))
}

/// Validator owner: create an active farm product + price.
pub async fn create_farm_product_and_price(
    staking: &near_workspaces::Account,
    pool: &near_workspaces::Account,
    validator_owner: &near_workspaces::Account,
    reward_rate: &str,
    amount_yocto: &str,
    max_amount_yocto: Option<&str>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let cp = validator_owner
        .call(staking.id(), "create_product")
        .args_json(json!({
            "validator_id": pool.id(),
            "name": "Fixture Farm Product",
            "description": "sandbox-farm",
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cp.is_success(), "create_product: {:#?}", cp.outcomes());
    let product_id: String = cp.into_result()?.json()?;

    let cpr = validator_owner
        .call(staking.id(), "create_price")
        .args_json(json!({
            "product_id": product_id,
            "name": "Farm",
            "description": "",
            "amount": amount_yocto,
            "price_type": "Farm",
            // TODO: consider moving billing_period and lock_factor_near_months into metadata
            // in a future catalog cleanup.
            "billing_period": null,
            "lock_factor_near_months": "0",
            "metadata": {
                "max_amount": max_amount_yocto,
                "farm_reward_rate": reward_rate,
            },
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cpr.is_success(), "create_price: {:#?}", cpr.outcomes());
    let price_id: String = cpr.into_result()?.json()?;
    Ok((product_id, price_id))
}

/// Validator owner: add another active recurring-monthly price on an existing product.
pub async fn create_recurring_price_on_product(
    staking: &near_workspaces::Account,
    validator_owner: &near_workspaces::Account,
    product_id: &str,
    name: &str,
    amount_yocto: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let cpr = validator_owner
        .call(staking.id(), "create_price")
        .args_json(json!({
            "product_id": product_id,
            "name": name,
            "description": "",
            "amount": amount_yocto,
            "price_type": "Recurring",
            "billing_period": "Monthly",
            "lock_factor_near_months": LOCK_FACTOR_DENOM,
            "metadata": null,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(200))
        .transact()
        .await?;
    assert!(cpr.is_success(), "create_price: {:#?}", cpr.outcomes());
    Ok(cpr.into_result()?.json()?)
}

pub async fn buyer_update_subscription_with_stake_increase(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    subscription_id: &str,
    target_price_id: &str,
    target_amount_yocto: u128,
    deposit_near: u128,
) -> Result<String, Box<dyn std::error::Error>> {
    let outcome: serde_json::Value = buyer
        .call(staking_id, "update_subscription")
        .args_json(json!({
            "subscription_id": subscription_id,
            "target_price_id": target_price_id,
            "target_amount": target_amount_yocto.to_string(),
        }))
        .deposit(NearToken::from_near(deposit_near))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?
        .json()?;
    Ok(outcome["lock_id"]
        .as_str()
        .ok_or("missing lock_id in update_subscription outcome")?
        .to_string())
}

pub async fn buyer_update_subscription_scheduled(
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    subscription_id: &str,
    target_price_id: &str,
    target_amount_yocto: u128,
) -> Result<(), Box<dyn std::error::Error>> {
    buyer
        .call(staking_id, "update_subscription")
        .args_json(json!({
            "subscription_id": subscription_id,
            "target_price_id": target_price_id,
            "target_amount": target_amount_yocto.to_string(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(WsGas::from_tgas(SETTLEMENT_PIPELINE_GAS_TGAS))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

/// Advance `epoch_count` NEAR epochs and run `epoch_settle` each time (pool stake / unstake / withdraw steps).
pub async fn drive_validator_settlement_epochs(
    worker: &Worker<Sandbox>,
    caller: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    pool_id: &near_workspaces::AccountId,
    epoch_count: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..epoch_count {
        fast_forward_until_epoch_delta(worker, 1, Some(caller), Some(staking_id)).await?;
        call_epoch_settle(caller, staking_id, pool_id)
            .await?
            .into_result()?;
    }
    Ok(())
}

/// Wait until `lock.end_ns`, then call `unlock`.
pub async fn unlock_lock_after_expiry(
    worker: &Worker<Sandbox>,
    buyer: &near_workspaces::Account,
    staking_id: &near_workspaces::AccountId,
    lock_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let lock: serde_json::Value = worker
        .view(staking_id, "get_lock")
        .args_json(json!({ "lock_id": lock_id }))
        .await?
        .json()?;
    let end_ns = json_u64_field(&lock["end_ns"]).expect("lock.end_ns");
    fast_forward_until_timestamp(
        worker,
        end_ns.saturating_add(1),
        Some(buyer),
        Some(staking_id),
    )
    .await?;
    buyer_unlock(buyer, staking_id, lock_id).await
}
