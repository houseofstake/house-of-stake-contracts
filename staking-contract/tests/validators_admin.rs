//! Validator allowlist: listing and removal when idle.

mod common;

use common::{
    BUYER, CATALOG_MANAGER, OWNER, POOL, VALIDATOR_OWNER_ACCOUNT, acct, add_validator_allowlisted,
    ctx, deploy, one_yocto, register_buyer, setup_catalog_near_oneoff,
    testing_env_catalog_callback,
};
use near_sdk::json_types::U64;
use near_sdk::{AccountId, NearToken, testing_env};
use staking_contract::types::ValidatorStatus;
use staking_contract::validators::{MAX_VALIDATOR_CATALOG_MANAGERS, MAX_VALIDATORS};

#[test]
fn get_validators_includes_registered_pool() {
    let mut c = deploy();

    add_validator_allowlisted(&mut c);

    let vs = c.get_validators(0, 10);
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0].validator_id, acct(POOL));
    assert_eq!(vs[0].status, ValidatorStatus::Active);
    assert!(vs[0].catalog_manager_account_ids.is_empty());
}

#[test]
fn remove_validator_on_idle_pool_marks_removed() {
    let mut c = deploy();

    add_validator_allowlisted(&mut c);

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.remove_validator(acct(POOL));

    let v = c.get_validator(acct(POOL)).expect("validator row retained");
    assert_eq!(v.status, ValidatorStatus::Removed);
}

#[test]
#[should_panic(expected = "Cannot remove this validator")]
fn remove_validator_fails_while_pending_stake_exists() {
    let mut c = deploy();
    let (_pid, price_id) = setup_catalog_near_oneoff(&mut c);
    register_buyer(&mut c);

    let dur = c.get_config().min_lock_duration_ns.0.saturating_add(10_000);
    testing_env!(ctx(acct(BUYER), NearToken::from_near(50)));
    let _ = c.lock(Some(price_id), None, Some(U64(dur)));

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.remove_validator(acct(POOL));
}

#[test]
#[should_panic(expected = "Validator limit reached")]
fn add_validator_rejects_after_max_validators() {
    let mut c = deploy();

    for index in 0..MAX_VALIDATORS {
        let validator_id: AccountId = format!("pool-{index}.testnet").parse().unwrap();
        testing_env!(ctx(acct(OWNER), one_yocto()));
        c.add_validator(validator_id);
    }

    testing_env!(ctx(acct(OWNER), one_yocto()));
    c.add_validator("pool-over-limit.testnet".parse().unwrap());
}

#[test]
fn validator_owner_can_add_and_remove_multiple_catalog_managers() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct("manager-two.near"),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let validator = c.get_validator(acct(POOL)).expect("validator");
    assert_eq!(
        validator.catalog_manager_account_ids,
        vec![acct(CATALOG_MANAGER), acct("manager-two.near")]
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.remove_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.remove_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct("manager-missing.near"),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let validator = c.get_validator(acct(POOL)).expect("validator");
    assert_eq!(
        validator.catalog_manager_account_ids,
        vec![acct("manager-two.near")]
    );
}

#[test]
#[should_panic(expected = "Only the validator owner can call this method")]
fn non_owner_cannot_add_validator_catalog_manager() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(CATALOG_MANAGER),
    );
}

#[test]
#[should_panic(expected = "Only the validator owner can call this method")]
fn catalog_manager_cannot_remove_validator_catalog_manager() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.remove_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct(CATALOG_MANAGER),
        acct(CATALOG_MANAGER),
    );
}

#[test]
#[should_panic(expected = "Validator catalog manager limit reached")]
fn add_validator_catalog_manager_rejects_after_max_managers() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    for index in 0..MAX_VALIDATOR_CATALOG_MANAGERS {
        testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
        c.add_validator_catalog_manager_after_get_owner(
            acct(VALIDATOR_OWNER_ACCOUNT),
            acct(POOL),
            format!("manager-{index}.near").parse().unwrap(),
            acct(VALIDATOR_OWNER_ACCOUNT),
        );
    }

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.add_validator_catalog_manager_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        acct("manager-over-limit.near"),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}
