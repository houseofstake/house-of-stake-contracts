//! `lock` guards.

mod common;

use common::{
    BUYER, OWNER, POOL, acct, add_subscription_price, ctx, ctx_ts, deploy, register_buyer,
    setup_catalog_near_oneoff, setup_catalog_near_subscription,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};

const BASE_TS: u64 = 1_700_000_000_000_000_000;

#[test]
#[should_panic(expected = "Lock duration is outside the allowed range")]
fn lock_rejects_duration_below_min() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_sub(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
#[should_panic(expected = "Lock duration is outside the allowed range")]
fn lock_rejects_duration_above_max() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().max_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
#[should_panic(expected = "Attached NEAR is below the contract minimum lock amount")]
fn lock_rejects_deposit_below_min_lock_amount() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
#[should_panic(expected = "This subscription period already has an active lock")]
fn lock_rejects_second_lock_same_period() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = c.lock(Some(price_id.clone()), None, None);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    c.lock(Some(price_id), None, None);
}

#[test]
#[should_panic(expected = "price_id must match current subscription tier")]
fn lock_rejects_wrong_tier_at_renewal_without_upgrade() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = c.lock(Some(price_low.clone()), None, None);

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    let renew_ts = sub.end_ns.0;

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), renew_ts));
    c.lock(Some(price_high), None, None);
}

#[test]
#[should_panic(expected = "This validator is paused or removed")]
fn lock_fails_when_validator_paused() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.pause_validator(acct(POOL));

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}
