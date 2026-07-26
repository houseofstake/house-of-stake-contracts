//! Ownership transfer, contract initialization, and owner-only config updates.

mod common;

use common::{
    BUYER, GUARDIAN, NEW_OWNER, OWNER, acct, base_config, ctx, deploy, deploy_with_config,
    one_yocto,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};

#[test]
#[should_panic(expected = "min_lock_amount must be at least 1 NEAR")]
fn new_rejects_min_lock_amount_below_protocol_floor() {
    let mut cfg = base_config();
    cfg.min_lock_amount = NearToken::from_millinear(999);

    let _ = deploy_with_config(cfg);
}

#[test]
fn propose_and_accept_ownership_transfers_owner() {
    let mut c = deploy();
    let new_owner = acct(NEW_OWNER);

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.propose_new_owner_account_id(Some(new_owner.clone()));

    assert_eq!(
        c.get_config().proposed_new_owner_account_id.as_ref(),
        Some(&new_owner)
    );

    testing_env!(ctx(new_owner.clone(), one_yocto()));
    c.accept_ownership();

    assert_eq!(c.get_config().owner_account_id, new_owner);
    assert!(c.get_config().proposed_new_owner_account_id.is_none());
}

#[test]
fn owner_can_clear_proposed_owner() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.propose_new_owner_account_id(Some(acct(NEW_OWNER)));

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.propose_new_owner_account_id(None);

    assert!(c.get_config().proposed_new_owner_account_id.is_none());
}

#[test]
#[should_panic(expected = "Only the proposed new owner can call this method")]
fn accept_ownership_rejects_wrong_account() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.propose_new_owner_account_id(Some(acct(NEW_OWNER)));

    testing_env!(ctx(acct(BUYER), one_yocto()));
    c.accept_ownership();
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn accept_ownership_requires_one_yocto() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.propose_new_owner_account_id(Some(acct(NEW_OWNER)));

    testing_env!(ctx(acct(NEW_OWNER), NearToken::from_yoctonear(0)));
    c.accept_ownership();
}

#[test]
#[should_panic(expected = "Only the contract owner can call this method")]
fn non_owner_cannot_set_guardians() {
    let mut c = deploy();

    testing_env!(ctx(acct(BUYER), one_yocto()));
    c.set_guardians(vec![acct(GUARDIAN)]);
}

#[test]
fn owner_sets_epoch_unstake_and_config_round_trips() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_epoch_unstake_settle_epochs(11);

    assert_eq!(c.get_config().epoch_unstake_settle_epochs, 11);

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_per_lock_storage_stake(NearToken::from_millinear(1));

    assert_eq!(
        c.get_config().per_lock_storage_stake,
        NearToken::from_millinear(1)
    );

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_per_farm_position_storage_stake(NearToken::from_millinear(3));

    assert_eq!(
        c.get_config().per_farm_position_storage_stake,
        NearToken::from_millinear(3)
    );

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_per_purchase_storage_stake(NearToken::from_millinear(2));

    assert_eq!(
        c.get_config().per_purchase_storage_stake,
        NearToken::from_millinear(2)
    );

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_min_storage_deposit(NearToken::from_millinear(200));

    assert_eq!(
        c.get_config().min_storage_deposit,
        NearToken::from_millinear(200)
    );

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_min_lock_amount(NearToken::from_near(2));

    assert_eq!(c.get_config().min_lock_amount, NearToken::from_near(2));

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_min_lock_amount(NearToken::from_near(1));
    assert_eq!(c.get_config().min_lock_amount, NearToken::from_near(1));

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_lock_bounds(U64(10), U64(1_000_000_000_000_000));

    assert_eq!(c.get_config().min_lock_duration_ns.0, 10);
    assert_eq!(c.get_config().max_lock_duration_ns.0, 1_000_000_000_000_000);
}

#[test]
#[should_panic(expected = "min_lock_amount must be at least 1 NEAR")]
fn set_min_lock_amount_rejects_below_protocol_floor() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_min_lock_amount(NearToken::from_millinear(500));
}

#[test]
#[should_panic(expected = "Minimum lock duration cannot exceed maximum lock duration")]
fn set_lock_bounds_rejects_inverted_range() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_lock_bounds(U64(10), U64(9));
}

/// Guardian (not owner) may pause when listed in `guardians`.
#[test]
fn guardian_can_pause() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_guardians(vec![acct(GUARDIAN)]);

    testing_env!(ctx(acct(GUARDIAN), one_yocto()));
    c.pause();
    assert!(c.is_paused());
}

#[test]
#[should_panic(expected = "Only a guardian or the contract owner can call this method")]
fn random_account_cannot_pause() {
    let mut c = deploy();

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.set_guardians(vec![acct(GUARDIAN)]);

    testing_env!(ctx(acct(BUYER), one_yocto()));
    c.pause();
}
