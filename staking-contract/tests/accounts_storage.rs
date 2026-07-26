//! `storage_withdraw` retention and bounds.

mod common;

use common::{
    BUYER, POOL, acct, base_config, ctx, deploy_with_config, one_yocto, register_buyer,
    setup_catalog_near_oneoff,
};
use near_sdk::{NearToken, testing_env};
use staking_contract::PendingUnstakeTranche;

#[test]
fn storage_balance_of_returns_none_for_unregistered_account() {
    let c = deploy_with_config(base_config());

    assert!(c.storage_balance_of(acct(BUYER)).is_none());
}

#[test]
fn storage_balance_bounds_uses_min_storage_deposit() {
    let mut cfg = base_config();
    cfg.min_storage_deposit = NearToken::from_millinear(123);
    let c = deploy_with_config(cfg);

    let bounds = c.storage_balance_bounds();
    assert_eq!(bounds.min, NearToken::from_millinear(123));
    assert_eq!(bounds.max, None);
}

#[test]
fn storage_deposit_returns_updated_balance() {
    let mut c = deploy_with_config(base_config());

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(200)));
    let balance = c.storage_deposit(None, None);

    assert_eq!(balance.total, NearToken::from_millinear(200));
    assert_eq!(balance.available, NearToken::from_millinear(100));
    let stored = c.storage_balance_of(acct(BUYER)).expect("registered");
    assert_eq!(stored.total, balance.total);
    assert_eq!(stored.available, balance.available);
}

#[test]
fn storage_deposit_registration_only_accepts_minimum_needed() {
    let mut c = deploy_with_config(base_config());

    testing_env!(ctx(acct(BUYER), NearToken::from_near(1)));
    let balance = c.storage_deposit(None, Some(true));

    assert_eq!(balance.total, NearToken::from_millinear(100));
    assert_eq!(balance.available, NearToken::from_yoctonear(0));
}

#[test]
fn storage_withdraw_none_withdraws_available_only() {
    let mut c = deploy_with_config(base_config());

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(200)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let balance = c.storage_withdraw(None);

    assert_eq!(balance.total, NearToken::from_millinear(100));
    assert_eq!(balance.available, NearToken::from_yoctonear(0));
}

#[test]
#[should_panic(expected = "Withdraw exceeds available storage")]
fn storage_withdraw_rejects_amount_above_available() {
    let mut c = deploy_with_config(base_config());
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.storage_withdraw(Some(NearToken::from_near(1_000)));
}

#[test]
#[should_panic(expected = "Withdraw exceeds available storage")]
fn storage_withdraw_rejects_dropping_below_required_retention() {
    let mut cfg = base_config();
    cfg.per_lock_storage_stake = NearToken::from_millinear(50);
    cfg.min_storage_deposit = NearToken::from_millinear(100);

    let mut c = deploy_with_config(cfg);
    let (_pid, price_id) = common::setup_catalog_near_oneoff(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(200)));
    c.storage_deposit(None, None);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(1);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let _ = c.lock(Some(price_id), None, Some(near_sdk::json_types::U64(dur)));

    // After one lock, required prepaid is min 100m + 50m × 1 lock = 150m; withdrawing 60m would drop below.
    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.storage_withdraw(Some(NearToken::from_millinear(60)));
}

#[test]
#[should_panic(expected = "Withdraw exceeds available storage")]
fn storage_withdraw_rejects_dropping_below_purchase_retention() {
    let mut cfg = base_config();
    cfg.per_purchase_storage_stake = NearToken::from_millinear(50);
    cfg.min_storage_deposit = NearToken::from_millinear(100);

    let mut c = deploy_with_config(cfg);
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(200)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.pay(Some(price_id), None, near_sdk::json_types::U64(1));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.storage_withdraw(Some(NearToken::from_millinear(60)));
}

#[test]
fn per_purchase_storage_top_up_is_visible_as_available_balance() {
    let mut cfg = base_config();
    cfg.per_purchase_storage_stake = NearToken::from_millinear(50);
    cfg.min_storage_deposit = NearToken::from_millinear(100);

    let mut c = deploy_with_config(cfg);
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(150)));
    c.storage_deposit(None, None);
    assert_eq!(
        c.storage_balance_of(acct(BUYER))
            .expect("registered")
            .available,
        NearToken::from_millinear(50)
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.pay(Some(price_id), None, near_sdk::json_types::U64(1));
    assert_eq!(
        c.storage_balance_of(acct(BUYER))
            .expect("registered")
            .available,
        NearToken::from_yoctonear(0)
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(50)));
    let balance = c.storage_deposit(None, None);
    assert_eq!(balance.available, NearToken::from_millinear(50));
}

#[test]
fn storage_unregister_refunds_simple_registration() {
    let mut c = deploy_with_config(base_config());

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(100)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    assert!(c.storage_unregister(None));
    assert!(c.storage_balance_of(acct(BUYER)).is_none());
}

#[test]
#[should_panic(expected = "Force unregister is not supported")]
fn storage_unregister_rejects_force_true() {
    let mut c = deploy_with_config(base_config());

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(100)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.storage_unregister(Some(true));
}

#[test]
fn storage_unregister_returns_false_when_retained_storage_exists() {
    let mut cfg = base_config();
    cfg.per_purchase_storage_stake = NearToken::from_millinear(50);
    cfg.min_storage_deposit = NearToken::from_millinear(100);

    let mut c = deploy_with_config(cfg);
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_millinear(150)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.pay(Some(price_id), None, near_sdk::json_types::U64(1));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    assert!(!c.storage_unregister(None));
    assert!(c.storage_balance_of(acct(BUYER)).is_some());
}

#[test]
fn storage_unregister_returns_false_with_pending_unstake_tranches() {
    let mut c = deploy_with_config(base_config());
    common::add_validator_allowlisted(&mut c);
    register_buyer(&mut c);

    c.user_pending_unstake.insert(
        (acct(BUYER), acct(POOL)),
        vec![PendingUnstakeTranche {
            amount: NearToken::from_near(1),
            available_epoch_height: 0,
        }],
    );
    c.user_pending_unstake_validator_count
        .insert(acct(BUYER), 1);

    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));
    assert!(c.storage_balance_of(acct(BUYER)).is_some());
}
