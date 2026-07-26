//! Archived catalog entries reject new locks.

mod common;

use common::{
    BUYER, VALIDATOR_OWNER_ACCOUNT, acct, ctx, deploy, register_buyer, setup_catalog_near_oneoff,
    testing_env_catalog_callback,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};

#[test]
#[should_panic(expected = "This price is not active")]
fn lock_rejects_archived_price() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
#[should_panic(expected = "This product is not active")]
fn lock_rejects_archived_product() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}
