//! Negative tests for [`staking_contract::unlock::Contract::unlock`].

mod common;

use common::{
    BUYER, OWNER, acct, ctx_ts, deploy, one_yocto, register_buyer, setup_catalog_near_oneoff,
    setup_catalog_near_subscription, unwrap_sync_lock_id,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};
use staking_contract::utils::AVG_MONTH_NS;
#[test]
#[should_panic(expected = "Only the lock owner can unlock")]
fn unlock_rejects_wrong_predecessor() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(50_000);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    let lock = c.get_lock(lock_id.clone()).expect("lock");
    let end_ns = lock.end_ns.0;

    testing_env!(ctx_ts(acct(OWNER), one_yocto(), end_ns));
    c.unlock(lock_id);
}

#[test]
#[should_panic(expected = "Lock period has not ended yet")]
fn unlock_rejects_before_end_ns() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(50_000);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), start_ts.saturating_add(1)));
    c.unlock(lock_id);
}

#[test]
#[should_panic(expected = "Lock not found")]
fn unlock_rejects_unknown_lock_id() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(50_000);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    testing_env!(ctx_ts(
        acct(BUYER),
        one_yocto(),
        start_ts.saturating_add(dur).saturating_add(1)
    ));
    c.unlock("no-such-lock-id".into());
}

#[test]
#[should_panic(expected = "Active subscription lock cannot be unlocked")]
fn unlock_rejects_active_subscription_after_projected_renewal() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));

    let lock = c.get_lock(lock_id.clone()).expect("lock");
    testing_env!(ctx_ts(
        acct(BUYER),
        one_yocto(),
        lock.end_ns.0.saturating_add(1),
    ));
    c.unlock(lock_id);
}

#[test]
#[should_panic(expected = "Lock period has not ended yet")]
fn unlock_rejects_cancelled_subscription_until_projected_period_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    let late_ts = sub
        .end_ns
        .0
        .saturating_add((AVG_MONTH_NS.saturating_mul(2)) as u64);
    testing_env!(ctx_ts(acct(BUYER), one_yocto(), late_ts));
    c.cancel_subscription(sub.subscription_id);
    c.unlock(lock_id);
}

#[test]
#[ignore = "unlock returns Promise; second-call semantics covered in sandbox"]
#[should_panic(expected = "Lock is not active")]
fn unlock_rejects_second_call_after_unlock_requested() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let start_ts = 1_800_000_000_000_000_000_u64;
    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(50_000);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), start_ts));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    let lock = c.get_lock(lock_id.clone()).expect("lock");
    let end_ns = lock.end_ns.0;

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), end_ns));
    c.unlock(lock_id.clone());

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), end_ns.saturating_add(1)));
    c.unlock(lock_id);
}
