//! Catalog admin rules: `delete_product` / `delete_price` invariants.

mod common;

use common::{
    CATALOG_MANAGER, VALIDATOR_OWNER_ACCOUNT, acct, add_validator_allowlisted, ctx, deploy,
    setup_catalog_near_oneoff, testing_env_catalog_callback,
};
use near_sdk::json_types::U128;
use near_sdk::{NearToken, testing_env};
use staking_contract::types::{PriceMetadata, PriceType};
use staking_contract::utils::LOCK_FACTOR_DENOM;

#[test]
#[should_panic]
fn delete_product_fails_while_prices_attached() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
fn delete_product_succeeds_when_empty() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = c.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(common::POOL),
        "X".into(),
        "Y".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic]
fn delete_price_fails_when_in_use() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    common::register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(common::acct(common::BUYER), NearToken::from_near(50)));
    let _ = c.lock(
        Some(price_id.clone()),
        None,
        Some(near_sdk::json_types::U64(dur)),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "Price max_amount must be greater than or equal to amount")]
fn create_price_rejects_max_amount_below_amount() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = c.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(common::POOL),
        "X".into(),
        "Y".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        "Bad max".into(),
        "".into(),
        U128(10),
        PriceType::OneOff,
        None,
        U128(LOCK_FACTOR_DENOM),
        Some(PriceMetadata {
            max_amount: Some(U128(9)),
            farm_reward_rate: None,
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
fn validator_catalog_manager_can_manage_products_and_prices() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(common::POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = c.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(common::POOL),
        "Plan".into(),
        "Desc".into(),
        acct(CATALOG_MANAGER),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let price_id = c.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Price".into(),
        "".into(),
        U128(10),
        PriceType::OneOff,
        None,
        U128(LOCK_FACTOR_DENOM),
        None,
        acct(CATALOG_MANAGER),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.edit_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Edited plan".into(),
        "Edited desc".into(),
        acct(CATALOG_MANAGER),
    );
    assert_eq!(
        c.get_product(product_id.clone()).expect("product").name,
        "Edited plan"
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        Some("Edited price".into()),
        None,
        None,
        acct(CATALOG_MANAGER),
    );
    assert_eq!(
        c.get_price(price_id.clone()).expect("price").name,
        "Edited price"
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.set_product_default_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        Some(price_id.clone()),
        acct(CATALOG_MANAGER),
    );
    assert_eq!(
        c.get_product_default_price(product_id.clone()),
        Some(price_id.clone())
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        acct(CATALOG_MANAGER),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.unarchive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        acct(CATALOG_MANAGER),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        acct(CATALOG_MANAGER),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.unarchive_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        acct(CATALOG_MANAGER),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id,
        acct(CATALOG_MANAGER),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        acct(CATALOG_MANAGER),
    );
    assert!(c.get_product(product_id).is_none());
}

#[test]
#[should_panic(expected = "Only the validator owner or catalog manager can call this method")]
fn unrelated_account_cannot_manage_catalog() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(common::POOL),
        "Plan".into(),
        "Desc".into(),
        acct("stranger.near"),
    );
}
