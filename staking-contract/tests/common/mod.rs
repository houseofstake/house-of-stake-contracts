//! Shared NEAR `testing_env` helpers for host-side contract tests (`tests/*.rs`).

#![allow(dead_code)]

use near_sdk::json_types::{U64, U128};
use near_sdk::test_utils::{VMContextBuilder, get_logs};
use near_sdk::{
    AccountId, NearToken, PromiseOrValue, PromiseResult, RuntimeFeesConfig, VMContext, serde_json,
    test_vm_config, testing_env,
};
use staking_contract::types::{
    BillingPeriod, PriceId, PriceMetadata, PriceType, ProductId, SubscriptionId,
};
use staking_contract::utils::LOCK_FACTOR_DENOM;
use staking_contract::{Config, Contract, LockId};
use std::collections::HashMap;
use std::str::FromStr;

pub fn acct(s: &str) -> AccountId {
    AccountId::from_str(s).expect("valid account id")
}

pub fn event_json(event_name: &str) -> serde_json::Value {
    event_json_values(event_name)
        .into_iter()
        .next_back()
        .unwrap_or_else(|| panic!("missing EVENT_JSON event {event_name}"))
}

pub fn event_json_values(event_name: &str) -> Vec<serde_json::Value> {
    get_logs()
        .into_iter()
        .filter_map(|log| log.strip_prefix("EVENT_JSON:").map(str::to_owned))
        .filter_map(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .filter(|event| event["event"] == event_name)
        .collect()
}

pub const STAKING: &str = "staking.test.near";
pub const OWNER: &str = "owner.near";
pub const POOL: &str = "pool.near";
/// Account that owns the staking pool (`get_owner_id`), used when simulating catalog callbacks.
pub const VALIDATOR_OWNER_ACCOUNT: &str = "vowner.near";
pub const BUYER: &str = "buyer.near";
pub const NEW_OWNER: &str = "newowner.near";
pub const GUARDIAN: &str = "guardian.near";
/// Account used in catalog manager delegation tests; kept for catalog owner simulations.
pub const CATALOG_MANAGER: &str = "manager.near";

#[inline]
pub fn one_yocto() -> NearToken {
    NearToken::from_yoctonear(1)
}

/// `lock` returns [`PromiseOrValue`] on the WASM ABI. On **non-WASM**
/// targets (host `tests/*.rs`, `cargo check` on the host triple), the contract uses a synchronous mint path
/// so `testing_env!` does not need to resolve promise chains. Integration tests build the library without
/// `cfg(test)` but still hit that path via `not(target_arch = "wasm32")` in `lock.rs`.
pub fn unwrap_sync_lock_id(r: PromiseOrValue<LockId>) -> LockId {
    match r {
        PromiseOrValue::Value(id) => id,
        PromiseOrValue::Promise(_) => {
            panic!("unit tests expect synchronous lock (PromiseOrValue::Value)")
        }
    }
}

pub fn subscription_id_for_product(
    contract: &Contract,
    account_id: AccountId,
    product_id: ProductId,
) -> SubscriptionId {
    contract
        .get_subscription_for_product(account_id, product_id)
        .expect("subscription")
        .subscription_id
}

/// Baseline config; override fields in tests as needed.
pub fn base_config() -> Config {
    Config {
        owner_account_id: acct(OWNER),
        proposed_new_owner_account_id: None,
        guardians: vec![],
        min_lock_duration_ns: U64(1),
        max_lock_duration_ns: U64(u64::MAX / 8),
        epoch_unstake_settle_epochs: 4,
        min_storage_deposit: NearToken::from_millinear(100),
        per_lock_storage_stake: NearToken::from_near(0),
        per_farm_position_storage_stake: NearToken::from_near(0),
        per_purchase_storage_stake: NearToken::from_near(0),
        min_lock_amount: NearToken::from_near(1),
    }
}

pub fn ctx(pred: AccountId, attached: NearToken) -> VMContext {
    VMContextBuilder::new()
        .current_account_id(acct(STAKING))
        .predecessor_account_id(pred.clone())
        .signer_account_id(pred)
        .attached_deposit(attached)
        .account_balance(NearToken::from_near(500))
        .block_height(42)
        .epoch_height(100)
        .block_timestamp(1_700_000_000_000_000_000)
        .build()
}

pub fn ctx_ts(pred: AccountId, attached: NearToken, block_timestamp_ns: u64) -> VMContext {
    VMContextBuilder::new()
        .current_account_id(acct(STAKING))
        .predecessor_account_id(pred.clone())
        .signer_account_id(pred)
        .attached_deposit(attached)
        .account_balance(NearToken::from_near(500))
        .block_height(42)
        .epoch_height(100)
        .block_timestamp(block_timestamp_ns)
        .build()
}

/// Context for `#[private]` catalog callbacks (`*_after_get_owner`): contract calls itself.
fn ctx_catalog_callback() -> VMContext {
    ctx_catalog_callback_ts(1_700_000_000_000_000_000)
}

fn ctx_catalog_callback_ts(block_timestamp_ns: u64) -> VMContext {
    let id = acct(STAKING);
    VMContextBuilder::new()
        .current_account_id(id.clone())
        .predecessor_account_id(id.clone())
        .signer_account_id(id)
        .attached_deposit(NearToken::from_near(0))
        .account_balance(NearToken::from_near(500))
        .block_height(42)
        .epoch_height(100)
        .block_timestamp(block_timestamp_ns)
        .build()
}

/// Simulates `get_owner_id` resolving to `pool_owner` so `is_promise_success()` passes in callbacks.
pub fn testing_env_catalog_callback(pool_owner: AccountId) {
    testing_env_catalog_callback_at(pool_owner, 1_700_000_000_000_000_000);
}

pub fn testing_env_catalog_callback_at(pool_owner: AccountId, block_timestamp_ns: u64) {
    let payload = serde_json::to_vec(&pool_owner).expect("serialize pool owner AccountId");
    testing_env!(
        ctx_catalog_callback_ts(block_timestamp_ns),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(payload)],
    );
}

pub fn deploy_with_config(config: Config) -> Contract {
    Contract::new(config)
}

pub fn deploy() -> Contract {
    deploy_with_config(base_config())
}

/// Registers [`POOL`] on the allowlist (contract owner).
pub fn add_validator_allowlisted(contract: &mut Contract) {
    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    contract.add_validator(acct(POOL));
}

/// Owner registers pool; validator owner creates active catalog entries for NEAR one-off purchase.
pub fn setup_catalog_near_oneoff(contract: &mut Contract) -> (String, String) {
    add_validator_allowlisted(contract);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = contract.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        "Plan".into(),
        "Desc".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let price_id = contract.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Price".into(),
        "".into(),
        U128(1),
        PriceType::OneOff,
        None,
        U128(LOCK_FACTOR_DENOM),
        None,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    (product_id, price_id)
}

/// Recurring monthly NEAR price for subscription tests.
/// Extra recurring monthly price on an existing product (for tier upgrade/downgrade tests).
pub fn add_subscription_price(
    contract: &mut Contract,
    product_id: String,
    name: &str,
    amount_yocto: u128,
) -> String {
    add_subscription_price_with_metadata(contract, product_id, name, amount_yocto, None)
}

pub fn add_subscription_price_with_metadata(
    contract: &mut Contract,
    product_id: String,
    name: &str,
    amount_yocto: u128,
    metadata: Option<PriceMetadata>,
) -> String {
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    contract.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        name.into(),
        "".into(),
        U128(amount_yocto),
        PriceType::Recurring,
        Some(BillingPeriod::Monthly),
        U128(LOCK_FACTOR_DENOM),
        metadata,
        acct(VALIDATOR_OWNER_ACCOUNT),
    )
}

pub fn setup_catalog_near_subscription(contract: &mut Contract) -> (String, String) {
    add_validator_allowlisted(contract);

    add_subscription_product(contract, "Sub product", 1)
}

pub fn setup_catalog_farm(
    contract: &mut Contract,
    reward_rate: u128,
    min_amount_yocto: u128,
    max_amount_yocto: Option<u128>,
) -> (String, String) {
    add_validator_allowlisted(contract);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = contract.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        "Farm product".into(),
        "Farm desc".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let price_id = contract.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Farm".into(),
        "".into(),
        U128(min_amount_yocto),
        PriceType::Farm,
        None,
        U128(0),
        Some(PriceMetadata {
            max_amount: max_amount_yocto.map(U128),
            farm_reward_rate: Some(U128(reward_rate)),
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    (product_id, price_id)
}

pub fn add_subscription_product(
    contract: &mut Contract,
    name: &str,
    amount_yocto: u128,
) -> (String, String) {
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = contract.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        name.into(),
        "Desc".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let price_id = contract.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Monthly".into(),
        "".into(),
        U128(amount_yocto),
        PriceType::Recurring,
        Some(BillingPeriod::Monthly),
        U128(LOCK_FACTOR_DENOM),
        None,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    (product_id, price_id)
}

/// After creating a price, set it as the product default so `lock_*` can resolve `product_id` only.
pub fn set_default_price_for_product(
    contract: &mut Contract,
    product_id: ProductId,
    price_id: PriceId,
) {
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    contract.set_product_default_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        Some(price_id),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

pub fn register_buyer(contract: &mut Contract) {
    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(500)));
    contract.storage_deposit(None, None);
}
