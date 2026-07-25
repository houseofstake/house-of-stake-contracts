//! `EVENT_JSON` logs for indexers (NEP-297-style payload; standard `stake.dao`).

use crate::{PriceId, ProductId, ValidatorId};
use near_sdk::{AccountId, log};

fn emit(event: &str, data: serde_json::Value) {
    let payload = serde_json::json!({
        "standard": "stake.dao",
        "version": "1.0.0",
        "event": event,
        "data": data,
    });
    log!("EVENT_JSON:{}", payload);
}

pub fn log_validator_added(validator_id: &ValidatorId) {
    emit(
        "validator_add",
        serde_json::json!({
            "validator_id": validator_id.to_string(),
        }),
    );
}

pub fn log_validator_catalog_manager_add(
    validator_id: &ValidatorId,
    catalog_manager_account_id: &AccountId,
) {
    emit(
        "validator_catalog_manager_add",
        serde_json::json!({
            "validator_id": validator_id.to_string(),
            "catalog_manager_account_id": catalog_manager_account_id.to_string(),
        }),
    );
}

pub fn log_validator_catalog_manager_remove(
    validator_id: &ValidatorId,
    catalog_manager_account_id: &AccountId,
) {
    emit(
        "validator_catalog_manager_remove",
        serde_json::json!({
            "validator_id": validator_id.to_string(),
            "catalog_manager_account_id": catalog_manager_account_id.to_string(),
        }),
    );
}

pub fn log_product_created(product_id: &str, validator_id: &ValidatorId) {
    emit(
        "product_create",
        serde_json::json!({
            "product_id": product_id,
            "validator_id": validator_id.to_string(),
        }),
    );
}

pub fn log_subscription_cancel(account: &AccountId, product_id: &str) {
    emit(
        "subscription_cancel",
        serde_json::json!({
            "account_id": account.to_string(),
            "product_id": product_id,
        }),
    );
}

pub fn log_subscription_resume(account: &AccountId, product_id: &str) {
    emit(
        "subscription_resume",
        serde_json::json!({
            "account_id": account.to_string(),
            "product_id": product_id,
        }),
    );
}

pub fn log_subscription_update(account: &AccountId, target_price_id: &str, target_amount: u128) {
    emit(
        "subscription_update",
        serde_json::json!({
            "account_id": account.to_string(),
            "target_price_id": target_price_id,
            "target_amount": target_amount.to_string(),
        }),
    );
}

pub fn log_subscription_downgrade_prorate(account: &AccountId, product_id: &str, near_yocto: u128) {
    emit(
        "subscription_downgrade_prorate",
        serde_json::json!({
            "account_id": account.to_string(),
            "product_id": product_id,
            "near_yocto": near_yocto.to_string(),
        }),
    );
}

pub fn log_lock(lock_id: &str, account: &AccountId) {
    emit(
        "lock_create",
        serde_json::json!({
            "lock_id": lock_id,
            "account_id": account.to_string(),
        }),
    );
}

pub fn log_payment_create(
    purchase_id: &str,
    account: &AccountId,
    product_id: &str,
    price_id: &str,
    quantity: u64,
    amount_paid: u128,
) {
    emit(
        "payment_create",
        serde_json::json!({
            "purchase_id": purchase_id,
            "account_id": account.to_string(),
            "product_id": product_id,
            "price_id": price_id,
            "quantity": quantity.to_string(),
            "amount_paid": amount_paid.to_string(),
        }),
    );
}

pub fn log_revenue_withdraw(validator_id: &ValidatorId, account: &AccountId, amount: u128) {
    emit(
        "revenue_withdraw",
        serde_json::json!({
            "validator_id": validator_id.to_string(),
            "account_id": account.to_string(),
            "amount": amount.to_string(),
        }),
    );
}

pub fn log_unlock(lock_id: &str, account: &AccountId, validator_id: &ValidatorId) {
    emit(
        "unlock",
        serde_json::json!({
            "lock_id": lock_id,
            "account_id": account.to_string(),
            "validator_id": validator_id.to_string(),
        }),
    );
}

pub fn log_withdraw(account: &AccountId, validator_id: &ValidatorId, amount_yocto: u128) {
    emit(
        "withdraw",
        serde_json::json!({
            "account_id": account.to_string(),
            "validator_id": validator_id.to_string(),
            "amount_yocto": amount_yocto.to_string(),
        }),
    );
}

pub fn log_epoch_operation(epoch_action: &str, validator_id: &ValidatorId) {
    emit(
        epoch_action,
        serde_json::json!({
            "epoch_action": epoch_action,
            "validator_id": validator_id.to_string(),
        }),
    );
}

pub fn log_validator_withdraw_in(amount_yocto: u128, validator_id: &ValidatorId) {
    emit(
        "validator_withdraw_in",
        serde_json::json!({
            "validator_id": validator_id.to_string(),
            "amount_yocto": amount_yocto.to_string(),
        }),
    );
}

pub fn log_farm_stake(
    account: &AccountId,
    product_id: &ProductId,
    price_id: &PriceId,
    validator_id: &ValidatorId,
    amount_yocto: u128,
    shares: u128,
) {
    emit(
        "farm_stake",
        serde_json::json!({
            "account_id": account.to_string(),
            "product_id": product_id,
            "price_id": price_id,
            "validator_id": validator_id.to_string(),
            "amount_yocto": amount_yocto.to_string(),
            "shares": shares.to_string(),
        }),
    );
}

pub fn log_farm_unstake(
    account: &AccountId,
    product_id: &ProductId,
    price_id: &PriceId,
    validator_id: &ValidatorId,
    amount_yocto: u128,
    shares_removed: u128,
    remaining_shares: u128,
) {
    emit(
        "farm_unstake",
        serde_json::json!({
            "account_id": account.to_string(),
            "product_id": product_id,
            "price_id": price_id,
            "validator_id": validator_id.to_string(),
            "amount_yocto": amount_yocto.to_string(),
            "shares_removed": shares_removed.to_string(),
            "remaining_shares": remaining_shares.to_string(),
        }),
    );
}

pub fn log_farm_reward_rate_update(price_id: &PriceId, reward_rate: u128) {
    emit(
        "farm_reward_rate_update",
        serde_json::json!({
            "price_id": price_id,
            "reward_rate": reward_rate.to_string(),
        }),
    );
}
