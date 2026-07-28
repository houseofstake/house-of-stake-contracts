//! Direct one-off NEAR payment tests.

mod common;

use common::{
    BUYER, CATALOG_MANAGER, OWNER, POOL, VALIDATOR_OWNER_ACCOUNT, acct, base_config, ctx, deploy,
    register_buyer, set_default_price_for_product, setup_catalog_near_oneoff,
    setup_catalog_near_subscription, testing_env_catalog_callback,
};
use near_sdk::json_types::{U64, U128};
use near_sdk::{NearToken, testing_env};
use staking_contract::utils::LOCK_FACTOR_DENOM;
use staking_contract::{CatalogStatus, PriceType};

#[test]
fn pay_one_off_happy_path_records_purchase_and_revenue() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(3)));
    let purchase_id = c.pay(Some(price_id.clone()), None, U64(3));

    assert!(purchase_id.starts_with("pay_"));
    let purchase = c.get_purchase(purchase_id.clone()).expect("purchase");
    assert_eq!(purchase.purchase_id, purchase_id);
    assert_eq!(purchase.account_id, acct(BUYER));
    assert_eq!(purchase.product_id, product_id);
    assert_eq!(purchase.price_id, price_id.clone());
    assert_eq!(purchase.quantity, U64(3));
    assert_eq!(purchase.amount_paid, NearToken::from_yoctonear(3));

    assert_eq!(c.get_purchases(0, 10).len(), 1);
    assert_eq!(c.get_purchases_for_account(acct(BUYER), 0, 10).len(), 1);
    assert_eq!(
        c.get_purchases_for_product(purchase.product_id.clone(), 0, 10)
            .len(),
        1
    );
    assert_eq!(
        c.get_revenue_balance_for_validator(acct(POOL)),
        NearToken::from_yoctonear(3)
    );
    assert!(c.get_lock(purchase_id, None).is_none());

    let price = c.get_price(price_id).expect("price");
    assert_eq!(price.usage_count, 1);
    assert_eq!(price.status, CatalogStatus::Active);
}

#[test]
fn pay_indexes_purchases_with_nested_vectors() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let mut purchase_ids = Vec::new();
    for _ in 0..3 {
        testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
        purchase_ids.push(c.pay(Some(price_id.clone()), None, U64(1)));
    }

    let product_ids = c
        .purchases_by_product
        .get(&product_id)
        .expect("product purchase index");
    let account_ids = c
        .purchases_by_account
        .get(&acct(BUYER))
        .expect("account purchase index");
    assert_eq!(product_ids.len(), 3);
    assert_eq!(account_ids.len(), 3);
    assert_eq!(
        product_ids.get(0).expect("first product purchase"),
        &purchase_ids[0]
    );
    assert_eq!(
        product_ids.get(2).expect("third product purchase"),
        &purchase_ids[2]
    );
    assert_eq!(
        c.get_purchases_for_product(product_id, 1, 1)[0].purchase_id,
        purchase_ids[1]
    );
    assert_eq!(
        c.get_purchases_for_account(acct(BUYER), 2, 1)[0].purchase_id,
        purchase_ids[2]
    );
}

#[test]
fn pay_resolves_product_default_price() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    set_default_price_for_product(&mut c, product_id.clone(), price_id.clone());
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(2)));
    let purchase_id = c.pay(None, Some(product_id), U64(2));
    let purchase = c.get_purchase(purchase_id).expect("purchase");
    assert_eq!(purchase.price_id, price_id);
    assert_eq!(purchase.amount_paid, NearToken::from_yoctonear(2));
}

#[test]
#[should_panic(expected = "Provide only one of price_id or product_id")]
fn pay_rejects_both_price_and_product() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), Some(product_id), U64(1));
}

#[test]
#[should_panic(expected = "Provide price_id or product_id")]
fn pay_rejects_missing_price_and_product() {
    let mut c = deploy();
    setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(None, None, U64(1));
}

#[test]
#[should_panic(expected = "This price is not a one-off product price")]
fn pay_rejects_recurring_price() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_subscription(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
#[should_panic(expected = "Quantity must be greater than zero")]
fn pay_rejects_zero_quantity() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(0)));
    c.pay(Some(price_id), None, U64(0));
}

#[test]
#[should_panic(expected = "Attached deposit must equal price amount times quantity")]
fn pay_rejects_insufficient_deposit() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(2));
}

#[test]
#[should_panic(expected = "Attached deposit must equal price amount times quantity")]
fn pay_rejects_excess_deposit() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(3)));
    c.pay(Some(price_id), None, U64(2));
}

#[test]
#[should_panic(expected = "Top up storage for another purchase")]
fn pay_requires_prepaid_purchase_storage() {
    let mut config = base_config();
    config.per_purchase_storage_stake = NearToken::from_near(1);
    let mut c = staking_contract::Contract::new(config);
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
fn pay_storage_requirement_increases_after_purchase() {
    let mut config = base_config();
    config.per_purchase_storage_stake = NearToken::from_near(1);
    let mut c = staking_contract::Contract::new(config);
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(3)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id.clone()), None, U64(1));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
    assert_eq!(c.get_purchases_for_account(acct(BUYER), 0, 10).len(), 2);
}

#[test]
fn validator_owner_can_withdraw_full_revenue() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(5)));
    c.pay(Some(price_id), None, U64(5));

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.withdraw_revenue_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    assert_eq!(
        c.get_revenue_balance_for_validator(acct(POOL)),
        NearToken::from_yoctonear(0)
    );
    assert_eq!(c.get_purchases(0, 10).len(), 1);
}

#[test]
#[should_panic(expected = "The contract is paused; try again after it has been unpaused")]
fn revenue_withdraw_rejects_paused_contract() {
    let mut config = base_config();
    config.guardians = vec![acct(OWNER)];
    let mut c = staking_contract::Contract::new(config);
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(5)));
    c.pay(Some(price_id), None, U64(5));

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.pause();

    testing_env!(ctx(
        acct(VALIDATOR_OWNER_ACCOUNT),
        NearToken::from_yoctonear(1)
    ));
    c.withdraw_revenue(acct(POOL));
}

#[test]
#[should_panic(expected = "Only the validator owner can call this method")]
fn revenue_withdraw_rejects_non_owner() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(5)));
    c.pay(Some(price_id), None, U64(5));

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.withdraw_revenue_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct("not-owner.near"),
    );
}

#[test]
#[should_panic(expected = "Only the validator owner can call this method")]
fn validator_catalog_manager_cannot_withdraw_revenue() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(5)));
    c.pay(Some(price_id), None, U64(5));

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let _ = c.withdraw_revenue_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
    );
}

#[test]
#[should_panic(expected = "No revenue available to withdraw")]
fn revenue_withdraw_rejects_zero_balance() {
    let mut c = deploy();
    let (_product_id, _price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.withdraw_revenue_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "This price is not active; pick an active price")]
fn pay_rejects_archived_price() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
#[should_panic(expected = "This product is not active; pick an active product")]
fn pay_rejects_archived_product() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
#[should_panic(expected = "This validator is paused or removed")]
fn pay_rejects_paused_validator() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.pause_validator(acct(POOL));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
#[should_panic(expected = "This validator is paused or removed")]
fn pay_rejects_removed_validator() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env!(ctx(acct(OWNER), NearToken::from_yoctonear(1)));
    c.remove_validator(acct(POOL));

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(1)));
    c.pay(Some(price_id), None, U64(1));
}

#[test]
fn pay_uses_price_amount_times_quantity() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let price_id = c.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id.clone(),
        "Ten yocto".into(),
        "".into(),
        U128(10),
        PriceType::OneOff,
        None,
        U128(LOCK_FACTOR_DENOM),
        None,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_yoctonear(30)));
    let purchase_id = c.pay(Some(price_id), None, U64(3));
    let purchase = c.get_purchase(purchase_id).expect("purchase");
    assert_eq!(purchase.amount_paid, NearToken::from_yoctonear(30));
}
