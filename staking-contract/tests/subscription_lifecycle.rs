//! Cancel-at-period-end and unified subscription updates.

mod common;

use common::{
    BUYER, POOL, VALIDATOR_OWNER_ACCOUNT, acct, add_subscription_price,
    add_subscription_price_with_metadata, add_subscription_product, ctx, ctx_ts, deploy,
    event_json, event_json_values, register_buyer, setup_catalog_near_subscription,
    subscription_id_for_product, testing_env_catalog_callback, unwrap_sync_lock_id,
};
use near_sdk::NearToken;
use near_sdk::PromiseOrValue;
use near_sdk::json_types::U128;
use near_sdk::testing_env;
use staking_contract::types::PriceMetadata;
use staking_contract::types::TransactionStatus;
use staking_contract::types::{LockStatus, OrderRef, SubscriptionStatus};
use staking_contract::utils::AVG_MONTH_NS;

const BASE_TS: u64 = 1_700_000_000_000_000_000;
const JAN_1_2026_10_UTC_NS: u64 = 1_767_261_600_000_000_000;
const JAN_31_2026_10_UTC_NS: u64 = 1_769_853_600_000_000_000;
const FEB_1_2026_10_UTC_NS: u64 = 1_769_940_000_000_000_000;
const FEB_28_2026_10_UTC_NS: u64 = 1_772_272_800_000_000_000;
const MAR_1_2026_10_UTC_NS: u64 = 1_772_359_200_000_000_000;

#[test]
fn recurring_lock_uses_calendar_month_window_and_utc_anchor() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(50),
        JAN_31_2026_10_UTC_NS
    ));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub.start_ns.0, JAN_31_2026_10_UTC_NS);
    assert_eq!(sub.end_ns.0, FEB_28_2026_10_UTC_NS);
    assert_eq!(sub.anchor_day, 31);

    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(lock.start_ns, sub.start_ns);
    assert_eq!(lock.end_ns, sub.end_ns);
    match lock.order {
        OrderRef::Subscription {
            period_start_ns,
            period_end_ns,
            ..
        } => {
            assert_eq!(period_start_ns, sub.start_ns);
            assert_eq!(period_end_ns, sub.end_ns);
        }
        OrderRef::ProductPurchase { .. } => panic!("expected subscription order"),
    }
}

#[test]
fn recurring_lock_accepts_exact_monthly_amount_for_short_anchor_month() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_near_subscription(&mut c);
    let price_id = add_subscription_price(
        &mut c,
        product_id.clone(),
        "Exact monthly product",
        NearToken::from_near(1).as_yoctonear(),
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(1),
        JAN_31_2026_10_UTC_NS
    ));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub.start_ns.0, JAN_31_2026_10_UTC_NS);
    assert_eq!(sub.end_ns.0, FEB_28_2026_10_UTC_NS);
}

#[test]
fn recurring_renewal_accepts_same_amount_for_february_period() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_near_subscription(&mut c);
    let price_id = add_subscription_price(
        &mut c,
        product_id.clone(),
        "Exact monthly product",
        NearToken::from_near(1).as_yoctonear(),
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(1),
        JAN_1_2026_10_UTC_NS
    ));
    let first_lock = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(1),
        FEB_1_2026_10_UTC_NS
    ));
    let second_lock = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));
    assert_ne!(first_lock, second_lock);

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub.start_ns.0, FEB_1_2026_10_UTC_NS);
    assert_eq!(sub.end_ns.0, MAR_1_2026_10_UTC_NS);
}

#[test]
fn cancel_then_renew_after_period_opens_fresh_subscription() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_first = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id.clone());

    let event = event_json("subscription_cancel");
    assert_eq!(event["data"]["account_id"].as_str(), Some(BUYER));
    assert_eq!(
        event["data"]["subscription_id"].as_str(),
        Some(subscription_id.as_str())
    );
    assert_eq!(
        event["data"]["product_id"].as_str(),
        Some(product_id.as_str())
    );
    assert_eq!(event["data"]["price_id"].as_str(), Some(price_id.as_str()));

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert!(sub.cancel_at_period_end);
    assert_eq!(c.subscription_ids.len(), 1);

    let renew_ts = sub.end_ns.0.saturating_add(1);
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), renew_ts));
    let lock_second = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    assert_ne!(
        lock_first, lock_second,
        "renewal after cancelled period should mint a new lock"
    );

    let sub_after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert!(
        !sub_after.cancel_at_period_end,
        "new billing period should clear cancel-at-end"
    );
    assert_eq!(sub_after.status, SubscriptionStatus::Active);
    assert_ne!(
        sub_after.subscription_id, sub.subscription_id,
        "cancelled renewal should replace the stale subscription"
    );
    assert_eq!(
        c.subscription_ids.len(),
        1,
        "global index should replace the stale subscription, not grow"
    );
    assert_eq!(
        c.subscriptions_by_account
            .get(&acct(BUYER))
            .cloned()
            .unwrap_or_default(),
        vec![sub_after.subscription_id]
    );
}

#[test]
fn active_renewal_reuses_subscription_indexes() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let first_lock = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert_eq!(c.subscription_ids.len(), 1);

    let renew_ts = sub_before.end_ns.0.saturating_add(1);
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), renew_ts));
    let second_lock = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));
    assert_ne!(first_lock, second_lock);

    let sub_after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub_after.subscription_id, sub_before.subscription_id);
    assert_eq!(
        c.subscription_ids.len(),
        1,
        "renewing an active subscription must not duplicate the global index"
    );
    assert_eq!(
        c.subscriptions_by_account
            .get(&acct(BUYER))
            .cloned()
            .unwrap_or_default(),
        vec![sub_after.subscription_id]
    );
}

#[test]
fn subscription_listing_views_return_projected_records() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    let subscription = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let all_subscriptions = c.get_subscriptions(0, 10);
    assert_eq!(all_subscriptions.len(), 1);
    assert_eq!(
        all_subscriptions[0].subscription_id,
        subscription.subscription_id
    );
    let product_subscriptions = c.get_subscriptions_for_product(product_id.clone(), 0, 10);
    assert_eq!(product_subscriptions.len(), 1);
    assert_eq!(
        product_subscriptions[0].subscription_id,
        subscription.subscription_id
    );
    assert!(
        c.get_subscriptions_for_product(product_id, 1, 10)
            .is_empty()
    );
}

#[test]
fn resume_subscription_clears_cancel_before_period_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _lock_first = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id.clone());

    let sub_cancelled = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert!(sub_cancelled.cancel_at_period_end);
    let period_end_before = sub_cancelled.end_ns.0;

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.resume_subscription(subscription_id.clone());

    let event = event_json("subscription_resume");
    assert_eq!(event["data"]["account_id"].as_str(), Some(BUYER));
    assert_eq!(
        event["data"]["subscription_id"].as_str(),
        Some(subscription_id.as_str())
    );
    assert_eq!(
        event["data"]["product_id"].as_str(),
        Some(product_id.as_str())
    );
    assert_eq!(event["data"]["price_id"].as_str(), Some(price_id.as_str()));

    let sub_resumed = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert!(
        !sub_resumed.cancel_at_period_end,
        "resume should clear cancel-at-end within the same period"
    );
    assert_eq!(sub_resumed.end_ns.0, period_end_before);
    assert_eq!(sub_resumed.status, SubscriptionStatus::Active);
}

#[test]
#[should_panic(expected = "Current billing period has ended")]
fn resume_subscription_fails_after_period_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id.clone());

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let after_period_ts = sub.end_ns.0.saturating_add(1);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(1),
        after_period_ts
    ));
    c.resume_subscription(subscription_id);
}

#[test]
#[should_panic(expected = "Resume subscription before updating")]
fn update_subscription_rejects_cancel_at_period_end_subscription() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high), None, None));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.cancel_subscription(subscription_id.clone());

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        subscription_id,
        price_low,
        U128(NearToken::from_near(25).as_yoctonear()),
    );
}

#[test]
fn update_subscription_updates_tier_and_lock_amount() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_low = unwrap_sync_lock_id(c.lock(Some(price_low.clone()), None, None));
    let lock_before = c.get_lock(lock_low.clone()).expect("lock");
    let amt_before = lock_before.amount_near.as_yoctonear();

    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let subscription_id = sub_before.subscription_id.clone();
    let target_amount = NearToken::from_near(90).as_yoctonear();
    testing_env!(ctx(acct(BUYER), NearToken::from_near(40)));
    let outcome = c.update_subscription(
        subscription_id.clone(),
        price_high.clone(),
        U128(target_amount),
    );
    let event = event_json("subscription_update");
    assert_eq!(event["data"]["account_id"].as_str(), Some(BUYER));
    assert_eq!(
        event["data"]["subscription_id"].as_str(),
        Some(subscription_id.as_str())
    );
    assert_eq!(
        event["data"]["product_id"].as_str(),
        Some(product_id.as_str())
    );
    assert_eq!(
        event["data"]["target_price_id"].as_str(),
        Some(price_high.as_str())
    );
    assert_eq!(
        event["data"]["target_amount"].as_str(),
        Some(target_amount.to_string().as_str())
    );
    assert_eq!(
        event["data"]["update_kind"].as_str(),
        Some("changed_immediately")
    );

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub.price_id, price_high);
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host tests expect synchronous subscription update")
    };
    assert_eq!(outcome.kind, "changed_immediately");

    let lock_after = c.get_lock(lock_low).expect("lock");
    assert!(lock_after.amount_near.as_yoctonear() > amt_before);
    assert_eq!(lock_after.amount_near.as_yoctonear(), target_amount);
    assert_eq!(lock_after.status, LockStatus::Active);
}

#[test]
fn update_subscription_allows_different_product_on_same_validator() {
    let mut c = deploy();
    let (product_low, price_low) = setup_catalog_near_subscription(&mut c);
    let (product_high, price_high) = add_subscription_product(&mut c, "High product", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));
    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_low.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_near(40)));
    let _ = c.update_subscription(
        sub_before.subscription_id.clone(),
        price_high.clone(),
        U128(NearToken::from_near(90).as_yoctonear()),
    );

    assert!(
        c.get_subscription_for_product(acct(BUYER), product_low)
            .is_none(),
        "old product index should be removed after cross-product update"
    );
    let sub_after = c
        .get_subscription_for_product(acct(BUYER), product_high.clone())
        .expect("subscription moved to new product");
    assert_eq!(sub_after.subscription_id, sub_before.subscription_id);
    assert_eq!(sub_after.product_id, product_high);
    assert_eq!(sub_after.price_id, price_high);
}

#[test]
fn get_subscriptions_for_account_lists_all_owned_subscriptions() {
    let mut c = deploy();
    let (product_one, price_one) = setup_catalog_near_subscription(&mut c);
    let (product_two, price_two) = add_subscription_product(&mut c, "Second product", 2);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_one.clone()), None, None));

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(50),
        BASE_TS.saturating_add(1)
    ));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_two.clone()), None, None));

    let subs = c.get_subscriptions_for_account(acct(BUYER), 0, 10);
    assert_eq!(subs.len(), 2);
    assert!(
        subs.iter()
            .any(|sub| { sub.product_id == product_one && sub.price_id == price_one })
    );
    assert!(
        subs.iter()
            .any(|sub| { sub.product_id == product_two && sub.price_id == price_two })
    );
}

#[test]
#[should_panic(expected = "Target stake amount is above the price maximum")]
fn recurring_lock_rejects_amount_above_price_max_amount() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_near_subscription(&mut c);
    let capped_price = add_subscription_price_with_metadata(
        &mut c,
        product_id,
        "Capped",
        1,
        Some(PriceMetadata {
            max_amount: Some(U128(NearToken::from_near(60).as_yoctonear())),
            farm_reward_rate: None,
        }),
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(61), BASE_TS));
    c.lock(Some(capped_price), None, None);
}

#[test]
#[should_panic(expected = "Target stake amount is above the price maximum")]
fn update_subscription_rejects_target_amount_above_price_max_amount() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    let capped_price = add_subscription_price_with_metadata(
        &mut c,
        product_id.clone(),
        "Capped",
        1,
        Some(PriceMetadata {
            max_amount: Some(U128(NearToken::from_near(60).as_yoctonear())),
            farm_reward_rate: None,
        }),
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_near(11)));
    c.update_subscription(
        sub.subscription_id,
        capped_price,
        U128(NearToken::from_near(61).as_yoctonear()),
    );
}

#[test]
fn update_subscription_uses_projected_billing_window_after_stale_end_ns() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));

    let stored_end = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription")
        .end_ns
        .0;
    let past_period_ts = stored_end.saturating_add(1);
    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(40),
        past_period_ts
    ));

    let projected_end = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription")
        .end_ns
        .0;
    assert!(projected_end > past_period_ts);

    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let _ = c.update_subscription(
        sub_before.subscription_id,
        price_high.clone(),
        U128(NearToken::from_near(90).as_yoctonear()),
    );

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub.price_id, price_high);
    assert_eq!(sub.end_ns.0, projected_end);
    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(lock.start_ns, sub.start_ns);
    assert_eq!(lock.end_ns, sub.end_ns);
    match lock.order {
        OrderRef::Subscription {
            period_start_ns,
            period_end_ns,
            ..
        } => {
            assert_eq!(period_start_ns, sub.start_ns);
            assert_eq!(period_end_ns, sub.end_ns);
        }
        OrderRef::ProductPurchase { .. } => panic!("expected subscription order"),
    }
}

#[test]
#[should_panic(expected = "Cannot delete this price while it is in use")]
fn updated_subscription_price_is_marked_in_use() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    testing_env!(ctx(acct(BUYER), NearToken::from_near(40)));
    let _ = c.update_subscription(
        sub.subscription_id,
        price_high.clone(),
        U128(NearToken::from_near(90).as_yoctonear()),
    );

    let high = c.get_price(price_high.clone()).expect("high price");
    assert_eq!(high.usage_count, 1);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_high,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "This price is not active")]
fn update_callback_rejects_price_archived_after_entry_checks() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_high.clone(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let mut validator = c.get_validator(acct(POOL)).expect("validator");
    validator.tx_status = TransactionStatus::Busy;
    c.validators.insert(acct(POOL), validator.into());

    testing_env!(ctx(acct(common::STAKING), NearToken::from_near(0)));
    let _ = c.on_subscription_update_after_settle(
        acct(BUYER),
        NearToken::from_near(40),
        price_high,
        U128(NearToken::from_near(90).as_yoctonear()),
        sub.subscription_id,
        acct(POOL),
    );
}

#[test]
fn downgrade_projects_at_apply_time_without_manual_renewal_lock() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));

    let sub_high = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub_high.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert_eq!(
        sub.pending_update
            .as_ref()
            .and_then(|pending| pending.target_price_id.as_ref()),
        Some(&price_low)
    );

    let renew_ts = sub.end_ns.0.saturating_add(1);
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_yoctonear(0), renew_ts,));

    let sub_after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(sub_after.price_id, price_low);
    assert!(sub_after.pending_update.is_none());
    assert_eq!(sub_after.status, SubscriptionStatus::Active);
    assert!(
        c.user_pending_unstake
            .get(&(acct(BUYER), acct(POOL)))
            .is_none(),
        "view projection should not queue surplus until a later mutation applies cleanup"
    );
}

#[test]
fn cross_product_downgrade_projects_at_apply_time_without_manual_renewal_lock() {
    let mut c = deploy();
    let (product_low, price_low) = setup_catalog_near_subscription(&mut c);
    let (product_high, price_high) = add_subscription_product(&mut c, "High product", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_high.clone())
        .expect("subscription");
    let subscription_id = sub_before.subscription_id.clone();

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        subscription_id.clone(),
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_high.clone())
        .expect("subscription");
    assert_eq!(
        sub.pending_update
            .as_ref()
            .and_then(|pending| pending.target_price_id.as_ref()),
        Some(&price_low)
    );
    assert!(
        c.get_subscription_for_product(acct(BUYER), product_low.clone())
            .is_none(),
        "target product index should not move before renewal"
    );

    let renew_ts = sub.end_ns.0.saturating_add(1);
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_yoctonear(0), renew_ts,));

    assert!(
        c.get_subscription_for_product(acct(BUYER), product_high)
            .is_none(),
        "old product lookup should not return after cross-product downgrade is projected"
    );
    let sub_after = c
        .get_subscription_for_product(acct(BUYER), product_low.clone())
        .expect("subscription projected to target product");
    assert_eq!(sub_after.subscription_id, subscription_id);
    assert_eq!(sub_after.product_id, product_low);
    assert_eq!(sub_after.price_id, price_low);
    assert!(sub_after.pending_update.is_none());
    assert_eq!(sub_after.status, SubscriptionStatus::Active);
}

#[test]
#[should_panic(expected = "Subscription already exists for target product")]
fn cross_product_update_rejects_existing_target_product_subscription() {
    let mut c = deploy();
    let (_product_low, price_low) = setup_catalog_near_subscription(&mut c);
    let (product_high, price_high) = add_subscription_product(&mut c, "High product", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_low.clone()), None, None));

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(50),
        BASE_TS.saturating_add(1),
    ));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high), None, None));
    let sub_high = c
        .get_subscription_for_product(acct(BUYER), product_high)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub_high.subscription_id,
        price_low,
        U128(NearToken::from_near(25).as_yoctonear()),
    );
}

#[test]
#[should_panic(expected = "Subscription already has a pending update for target product")]
fn lock_rejects_product_reserved_by_pending_cross_product_update() {
    let mut c = deploy();
    let (_product_low, price_low) = setup_catalog_near_subscription(&mut c);
    let (product_high, price_high) = add_subscription_product(&mut c, "High product", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub_high = c
        .get_subscription_for_product(acct(BUYER), product_high)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub_high.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(50),
        BASE_TS.saturating_add(1),
    ));
    c.lock(Some(price_low), None, None);
}

#[test]
fn pending_update_projects_after_apply_time_without_manual_lock() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));

    let sub_high = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let apply_ns = sub_high.end_ns;
    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub_high.subscription_id.clone(),
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let scheduled = c
        .get_subscription(sub_high.subscription_id.clone())
        .expect("subscription");
    assert_eq!(
        scheduled
            .pending_update
            .as_ref()
            .and_then(|pending| pending.target_price_id.as_ref()),
        Some(&price_low)
    );
    assert_eq!(
        scheduled
            .pending_update
            .as_ref()
            .map(|pending| pending.apply_ns),
        Some(apply_ns)
    );

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(0),
        apply_ns.0.saturating_add(1),
    ));
    let projected = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert_eq!(projected.price_id, price_low);
    assert!(projected.pending_update.is_none());
    assert_eq!(projected.start_ns, apply_ns);
    assert!(projected.end_ns.0 > apply_ns.0);
    assert!(
        c.user_pending_unstake
            .get(&(acct(BUYER), acct(POOL)))
            .is_none()
    );

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(1),
        apply_ns.0.saturating_add(1),
    ));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id);
    let applied = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(applied.price_id, price_low);
    assert!(applied.pending_update.is_none());
    assert!(applied.cancel_at_period_end);
    assert!(
        c.user_pending_unstake
            .get(&(acct(BUYER), acct(POOL)))
            .is_none(),
        "cancel should not queue surplus unstake"
    );
}

#[test]
fn cross_product_pending_update_projects_under_target_product() {
    let mut c = deploy();
    let (product_low, price_low) = setup_catalog_near_subscription(&mut c);
    let (product_high, price_high) = add_subscription_product(&mut c, "High product", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub_before = c
        .get_subscription_for_product(acct(BUYER), product_high.clone())
        .expect("subscription");
    let subscription_id = sub_before.subscription_id.clone();

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        subscription_id.clone(),
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let scheduled = c
        .get_subscription(subscription_id.clone())
        .expect("subscription");
    let apply_ns = scheduled
        .pending_update
        .as_ref()
        .expect("pending update")
        .apply_ns;

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(0),
        apply_ns.0.saturating_add(1),
    ));
    assert!(
        c.get_subscription_for_product(acct(BUYER), product_high)
            .is_none(),
        "old product lookup should stop returning a due projected downgrade"
    );
    let projected = c
        .get_subscription_for_product(acct(BUYER), product_low.clone())
        .expect("subscription moved in projected view");
    assert_eq!(projected.subscription_id, subscription_id);
    assert_eq!(projected.product_id, product_low);
    assert_eq!(projected.price_id, price_low);
    assert!(projected.pending_update.is_none());
}

#[test]
fn immediate_update_clears_pending_update() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub.subscription_id.clone(),
        price_low,
        U128(NearToken::from_near(25).as_yoctonear()),
    );
    assert!(
        c.get_subscription(sub.subscription_id.clone())
            .expect("subscription")
            .pending_update
            .is_some()
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub.subscription_id.clone(),
        price_high.clone(),
        U128(NearToken::from_near(50).as_yoctonear()),
    );
    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(after.price_id, price_high);
    assert!(after.pending_update.is_none());
}

#[test]
fn plan_upgrade_with_stake_decrease_applies_plan_and_schedules_amount() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let outcome = c.update_subscription(
        sub.subscription_id,
        price_high.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(
        outcome.kind,
        "changed_immediately_and_scheduled_for_period_end"
    );
    assert!(outcome.immediate_plan_change);
    assert!(!outcome.pending_plan_change);
    assert_eq!(
        outcome.pending_stake_decrease,
        Some(U128(NearToken::from_near(25).as_yoctonear()))
    );

    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(after.price_id, price_high);
    let pending = after.pending_update.expect("pending amount decrease");
    assert!(pending.target_price_id.is_none());
    assert_eq!(
        pending.target_amount.expect("target amount").as_yoctonear(),
        NearToken::from_near(25).as_yoctonear()
    );

    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(
        lock.amount_near.as_yoctonear(),
        NearToken::from_near(50).as_yoctonear()
    );
}

#[test]
fn same_plan_stake_increase_applies_amount_only() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_near(25)));
    let outcome = c.update_subscription(
        sub.subscription_id,
        price_id,
        U128(NearToken::from_near(75).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(outcome.kind, "changed_immediately");
    assert!(!outcome.immediate_plan_change);
    assert_eq!(
        outcome.immediate_stake_increase,
        Some(U128(NearToken::from_near(25).as_yoctonear()))
    );
    assert!(!outcome.pending_plan_change);
    assert!(outcome.pending_stake_decrease.is_none());

    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(
        lock.amount_near.as_yoctonear(),
        NearToken::from_near(75).as_yoctonear()
    );
}

#[test]
#[should_panic(expected = "Attached NEAR is below the contract minimum lock amount")]
fn stake_increase_rejects_delta_below_min_lock_amount() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let sub = c
        .get_subscription_for_price(acct(BUYER), price_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub.subscription_id,
        price_id,
        U128(NearToken::from_near(50).as_yoctonear().saturating_add(1)),
    );
}

#[test]
fn plan_upgrade_same_stake_applies_plan_now() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_low), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let outcome = c.update_subscription(
        sub.subscription_id,
        price_high.clone(),
        U128(NearToken::from_near(50).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(outcome.kind, "changed_immediately");
    assert!(outcome.immediate_plan_change);
    assert!(outcome.immediate_stake_increase.is_none());
    assert!(!outcome.pending_plan_change);
    assert!(outcome.pending_stake_decrease.is_none());

    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(after.price_id, price_high);
    assert!(after.pending_update.is_none());
}

#[test]
fn plan_downgrade_with_stake_increase_stakes_now_and_schedules_plan() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_near(25)));
    let outcome = c.update_subscription(
        sub.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(75).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(
        outcome.kind,
        "changed_immediately_and_scheduled_for_period_end"
    );
    assert!(!outcome.immediate_plan_change);
    assert_eq!(
        outcome.immediate_stake_increase,
        Some(U128(NearToken::from_near(25).as_yoctonear()))
    );
    assert!(outcome.pending_plan_change);

    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert_eq!(after.price_id, price_high);
    let pending = after.pending_update.expect("pending plan downgrade");
    assert_eq!(pending.target_price_id.as_ref(), Some(&price_low));
    assert!(pending.target_amount.is_none());

    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(
        lock.amount_near.as_yoctonear(),
        NearToken::from_near(75).as_yoctonear()
    );
}

#[test]
fn same_plan_stake_decrease_schedules_amount_only() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let subscription_id = sub.subscription_id.clone();

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let outcome = c.update_subscription(
        subscription_id.clone(),
        price_id.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(outcome.kind, "scheduled_for_period_end");
    assert!(!outcome.pending_plan_change);
    assert_eq!(
        outcome.pending_stake_decrease,
        Some(U128(NearToken::from_near(25).as_yoctonear()))
    );

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(1),
        sub.end_ns.0.saturating_add(1),
    ));
    let _ = c.update_subscription(
        subscription_id.clone(),
        price_id.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let event = event_json_values("subscription_update")
        .into_iter()
        .find(|event| event["data"]["update_kind"].as_str() == Some("applied_pending_update"))
        .expect("applied pending update event");
    assert_eq!(event["data"]["account_id"].as_str(), Some(BUYER));
    assert_eq!(
        event["data"]["subscription_id"].as_str(),
        Some(subscription_id.as_str())
    );
    assert_eq!(
        event["data"]["product_id"].as_str(),
        Some(product_id.as_str())
    );
    assert_eq!(
        event["data"]["target_price_id"].as_str(),
        Some(price_id.as_str())
    );
    assert_eq!(
        event["data"]["target_amount"].as_str(),
        Some(NearToken::from_near(25).as_yoctonear().to_string().as_str())
    );
    assert_eq!(
        event["data"]["update_kind"].as_str(),
        Some("applied_pending_update")
    );

    let event = event_json("subscription_downgrade_prorate");
    assert_eq!(event["data"]["account_id"].as_str(), Some(BUYER));
    assert_eq!(
        event["data"]["subscription_id"].as_str(),
        Some(subscription_id.as_str())
    );
    assert_eq!(
        event["data"]["product_id"].as_str(),
        Some(product_id.as_str())
    );
    assert_eq!(event["data"]["price_id"].as_str(), Some(price_id.as_str()));
    assert_eq!(event["data"]["lock_id"].as_str(), Some(lock_id.as_str()));
    assert_eq!(
        event["data"]["near_yocto"].as_str(),
        Some(NearToken::from_near(25).as_yoctonear().to_string().as_str())
    );
}

#[test]
fn pending_only_update_syncs_lock_window_after_stale_end_ns() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let stored_end = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription")
        .end_ns
        .0;
    let past_period_ts = stored_end.saturating_add(1);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(1),
        past_period_ts,
    ));
    let projected = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    assert!(projected.end_ns.0 > past_period_ts);
    let _ = c.update_subscription(
        projected.subscription_id,
        price_id,
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(lock.start_ns, sub.start_ns);
    assert_eq!(lock.end_ns, sub.end_ns);
    match lock.order {
        OrderRef::Subscription {
            period_start_ns,
            period_end_ns,
            ..
        } => {
            assert_eq!(period_start_ns, sub.start_ns);
            assert_eq!(period_end_ns, sub.end_ns);
        }
        OrderRef::ProductPurchase { .. } => panic!("expected subscription order"),
    }
}

#[test]
fn cancel_clears_pending_update_without_applying_stake_decrease() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.update_subscription(
        sub.subscription_id,
        price_id,
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    let pending = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription")
        .pending_update
        .expect("pending update");
    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(1),
        pending.apply_ns.0.saturating_add(1),
    ));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id);

    let cancelled = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert!(cancelled.cancel_at_period_end);
    assert!(cancelled.pending_update.is_none());
    assert!(
        c.user_pending_unstake
            .get(&(acct(BUYER), acct(POOL)))
            .is_none(),
        "cancel should not apply a pending stake decrease"
    );
    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(
        lock.amount_near.as_yoctonear(),
        NearToken::from_near(50).as_yoctonear()
    );
}

#[test]
fn plan_downgrade_same_stake_schedules_plan_only() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high.clone()), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let outcome = c.update_subscription(
        sub.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(50).as_yoctonear()),
    );
    let PromiseOrValue::Value(outcome) = outcome else {
        panic!("host update should be synchronous");
    };
    assert_eq!(outcome.kind, "scheduled_for_period_end");
    assert!(outcome.pending_plan_change);
    assert!(outcome.pending_stake_decrease.is_none());
}

#[test]
#[should_panic(
    expected = "Cannot archive or delete this price while it is referenced by a pending subscription update"
)]
fn pending_update_target_price_blocks_archive() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.update_subscription(
        sub.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );
    let low = c.get_price(price_low.clone()).expect("low price");
    assert_eq!(low.usage_count, 0);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_low,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(
    expected = "Cannot archive or delete this price while it is referenced by a pending subscription update"
)]
fn pending_update_target_price_blocks_delete() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.update_subscription(
        sub.subscription_id,
        price_low.clone(),
        U128(NearToken::from_near(25).as_yoctonear()),
    );
    let low = c.get_price(price_low.clone()).expect("low price");
    assert_eq!(low.usage_count, 0);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.delete_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_low,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(
    expected = "Cannot archive or delete this product while it is referenced by a pending subscription update"
)]
fn pending_update_target_product_blocks_archive() {
    let mut c = deploy();
    let (product_id, price_low) = setup_catalog_near_subscription(&mut c);
    let price_high = add_subscription_price(&mut c, product_id.clone(), "High", 10);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_high), None, None));
    let sub = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let _ = c.update_subscription(
        sub.subscription_id,
        price_low,
        U128(NearToken::from_near(25).as_yoctonear()),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "Subscription not found")]
fn cancel_subscription_fails_without_subscription() {
    let mut c = deploy();
    let (_product_id, _) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.cancel_subscription("sub_missing".into());
}

#[test]
fn active_subscription_view_projects_next_cycle_after_period_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    let stored = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let stored_end = stored.end_ns.0;

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(0),
        stored_end.saturating_add(1),
    ));
    let projected_product = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    let projected_price = c
        .get_subscription_for_price(acct(BUYER), price_id)
        .expect("subscription");
    let projected_by_id = c
        .get_subscription(projected_product.subscription_id.clone())
        .expect("subscription");

    assert_eq!(projected_product.start_ns.0, stored_end);
    assert!(projected_product.end_ns.0 > stored_end);
    assert_eq!(projected_product.end_ns.0, projected_price.end_ns.0);
    assert_eq!(projected_product.end_ns.0, projected_by_id.end_ns.0);
}

#[test]
fn cancelled_subscription_view_does_not_project_after_period_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let _ = unwrap_sync_lock_id(c.lock(Some(price_id.clone()), None, None));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    let subscription_id = subscription_id_for_product(&c, acct(BUYER), product_id.clone());
    c.cancel_subscription(subscription_id);

    let cancelled = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");
    let cancelled_end = cancelled.end_ns.0;
    let subscription_id = cancelled.subscription_id.clone();
    assert_eq!(cancelled.status, SubscriptionStatus::Active);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(0),
        cancelled_end.saturating_add(1),
    ));
    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    let after_by_price = c
        .get_subscription_for_price(acct(BUYER), price_id)
        .expect("subscription");
    let after_by_id = c
        .get_subscription(subscription_id.clone())
        .expect("subscription");
    let after_for_account = c
        .get_subscriptions_for_account(acct(BUYER), 0, 10)
        .into_iter()
        .find(|sub| sub.subscription_id == subscription_id)
        .expect("subscription listed for account");

    assert!(after.cancel_at_period_end);
    assert_eq!(after.end_ns.0, cancelled_end);
    assert_eq!(after_by_price.end_ns.0, cancelled_end);
    assert_eq!(after.status, SubscriptionStatus::Expired);
    assert_eq!(after_by_price.status, SubscriptionStatus::Expired);
    assert_eq!(after_by_id.status, SubscriptionStatus::Expired);
    assert_eq!(after_for_account.status, SubscriptionStatus::Expired);
}

#[test]
fn cancel_subscription_normalizes_stale_window_before_marking_cancel_at_end() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(50), BASE_TS));
    let lock_id = unwrap_sync_lock_id(c.lock(Some(price_id), None, None));
    let initial = c
        .get_subscription_for_product(acct(BUYER), product_id.clone())
        .expect("subscription");

    let late_ts = initial
        .end_ns
        .0
        .saturating_add((AVG_MONTH_NS.saturating_mul(2)) as u64);
    testing_env!(ctx_ts(acct(BUYER), NearToken::from_yoctonear(1), late_ts));
    c.cancel_subscription(initial.subscription_id);

    let after = c
        .get_subscription_for_product(acct(BUYER), product_id)
        .expect("subscription");
    assert!(
        after.end_ns.0 > late_ts,
        "cancel-at-period-end should use current virtual cycle, not stale stored end_ns"
    );
    assert!(after.cancel_at_period_end);

    let lock = c.get_lock(lock_id).expect("lock");
    assert_eq!(lock.start_ns, after.start_ns);
    assert_eq!(lock.end_ns, after.end_ns);
    match lock.order {
        OrderRef::Subscription {
            period_start_ns,
            period_end_ns,
            ..
        } => {
            assert_eq!(period_start_ns, after.start_ns);
            assert_eq!(period_end_ns, after.end_ns);
        }
        OrderRef::ProductPurchase { .. } => panic!("expected subscription order"),
    }
}
