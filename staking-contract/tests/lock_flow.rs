//! Unified lock paths: pause gate, storage registration, one-off vs subscription.

mod common;

use common::{
    BUYER, OWNER, acct, ctx, ctx_ts, deploy, register_buyer, setup_catalog_near_oneoff,
    setup_catalog_near_subscription, unwrap_sync_lock_id,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};
use staking_contract::types::{CatalogStatus, LockStatus, OrderRef};

const OTHER_BUYER: &str = "other.near";

#[test]
fn lock_one_off_happy_path_records_lock_and_usage() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, Some(U64(dur))));

    let lock = c.get_lock(lock_id.clone(), None).expect("lock");
    assert_eq!(lock.account_id, acct(BUYER));
    assert_eq!(lock.status, LockStatus::Active);
    match &lock.order {
        OrderRef::ProductPurchase { price_id: p, .. } => assert_eq!(p, &price_id),
        _ => panic!("expected product order"),
    }

    let pr = c.get_price(price_id).expect("price");
    assert_eq!(pr.usage_count, 1);
    assert_eq!(pr.status, CatalogStatus::Active);
}

#[test]
fn get_locks_for_account_lists_one_off_locks_in_creation_order() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let first = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, Some(U64(dur))));
    testing_env!(ctx(acct(BUYER), NearToken::from_near(60)));
    let second = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    let locks = c.get_locks_for_account(acct(BUYER), 0, 10, None);
    assert_eq!(
        locks
            .iter()
            .map(|lock| lock.lock_id.clone())
            .collect::<Vec<_>>(),
        vec![first.clone(), second.clone()]
    );
    assert_eq!(
        c.get_locks_for_account(acct(BUYER), 1, 1, None)[0].lock_id,
        second
    );
    assert!(
        c.get_locks_for_account(acct(OTHER_BUYER), 0, 10, None)
            .is_empty()
    );

    let all_locks = c.get_locks(0, 10, None);
    assert_eq!(
        all_locks
            .iter()
            .map(|lock| lock.lock_id.clone())
            .collect::<Vec<_>>(),
        vec![first, second]
    );
}

#[test]
#[should_panic]
fn lock_one_off_fails_without_storage_deposit() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
#[should_panic]
fn lock_one_off_fails_when_paused() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.pause();

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
fn lock_recurring_creates_subscription_row() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    let sub = c
        .get_subscription_for_price(acct(BUYER), price_id)
        .expect("subscription");
    assert_eq!(sub.last_lock_id, lock_id);
    assert_eq!(
        sub.status,
        staking_contract::types::SubscriptionStatus::Active
    );
}

#[test]
fn get_locks_for_account_lists_recurring_subscription_locks() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    let locks = c.get_locks_for_account(acct(BUYER), 0, 10, Some(true));
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].lock_id, lock_id);
    assert_eq!(locks[0].account_id, acct(BUYER));
    match &locks[0].order {
        OrderRef::Subscription { price_id: p, .. } => assert_eq!(p, &price_id),
        _ => panic!("expected subscription order"),
    }
}

#[test]
fn get_locks_for_account_keeps_historical_subscription_renewals() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let first = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let first_end_ns = c
        .get_lock(first.clone(), None)
        .expect("first lock")
        .end_ns
        .0;

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(50),
        first_end_ns.saturating_add(1)
    ));
    let second = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));

    let locks = c.get_locks_for_account(acct(BUYER), 0, 10, None);
    assert_eq!(
        locks
            .iter()
            .map(|lock| lock.lock_id.clone())
            .collect::<Vec<_>>(),
        vec![first, second]
    );
}

#[test]
#[should_panic(expected = "duration_ns is required for one-off prices")]
fn lock_one_off_rejects_missing_duration() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, None);
}

#[test]
#[should_panic(expected = "duration_ns must be omitted for recurring subscription prices")]
fn lock_recurring_rejects_duration() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    c.lock(Some(price_id), None, Some(U64(dur)));
}

#[test]
fn get_config_round_trips() {
    let c = deploy();
    assert!(!c.is_paused());
    assert_eq!(c.get_config().owner_account_id, acct(OWNER));
    assert_eq!(c.get_config().epoch_unstake_settle_epochs, 4);
}

#[test]
fn unpause_allows_lock_again() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.pause();
    assert!(c.is_paused());

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.unpause();
    assert!(!c.is_paused());

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));
}

#[test]
fn user_lock_count_increments_on_lock() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, Some(U64(dur))));

    let n = c.user_lock_count.get(&acct(BUYER)).copied().expect("count");
    assert_eq!(n, 1);
}
