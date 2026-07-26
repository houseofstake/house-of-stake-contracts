//! Pipeline 6 `on_epoch_pipeline_release_with_lock_id`: Busy release and payable refund on tail failure.

mod common;

use common::{BUYER, POOL, STAKING, acct, ctx, deploy, setup_catalog_near_oneoff};
use near_sdk::{NearToken, PromiseError, PromiseOrValue, testing_env};
use staking_contract::types::{OrderRef, TransactionStatus, UserAction};

#[test]
fn lock_pipeline_tail_failure_releases_busy_and_schedules_refund() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    let pool = acct(POOL);

    let mut validator = c.get_validator(pool.clone()).expect("validator").clone();
    validator.tx_status = TransactionStatus::Busy;
    c.validators.insert(pool.clone(), validator.into());

    let locked = NearToken::from_near(50);
    let cont = UserAction::CommitLock {
        validator_id: pool.clone(),
        buyer: acct(BUYER),
        locked,
        duration_ns: 1_000,
        order: OrderRef::ProductPurchase {
            product_id,
            price_id,
        },
    };

    testing_env!(ctx(acct(STAKING), NearToken::from_near(0)));
    let out =
        c.on_epoch_pipeline_release_with_lock_id(Err(PromiseError::Failed), pool.clone(), cont);

    match out {
        PromiseOrValue::Promise(_) => {}
        PromiseOrValue::Value(_) => panic!("tail failure must schedule refund, not return lock id"),
    }

    let v = c.get_validator(pool).expect("validator");
    assert_eq!(
        v.tx_status,
        TransactionStatus::Idle,
        "tail failure must not leave validator Busy after release/refund path"
    );
}

#[test]
fn lock_pipeline_tail_success_releases_busy_and_returns_lock_id() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_near_oneoff(&mut c);
    let pool = acct(POOL);

    let mut validator = c.get_validator(pool.clone()).expect("validator").clone();
    validator.tx_status = TransactionStatus::Busy;
    c.validators.insert(pool.clone(), validator.into());

    let lock_id = "lock-test-1".to_string();
    let cont = UserAction::CommitLock {
        validator_id: pool.clone(),
        buyer: acct(BUYER),
        locked: NearToken::from_near(50),
        duration_ns: 1_000,
        order: OrderRef::ProductPurchase {
            product_id,
            price_id,
        },
    };

    testing_env!(ctx(acct(STAKING), NearToken::from_near(0)));
    let out = c.on_epoch_pipeline_release_with_lock_id(Ok(lock_id.clone()), pool.clone(), cont);

    match out {
        PromiseOrValue::Value(id) => assert_eq!(id, lock_id),
        PromiseOrValue::Promise(_) => panic!("successful tail must return lock id synchronously"),
    }

    let v = c.get_validator(pool).expect("validator");
    assert_eq!(v.tx_status, TransactionStatus::Idle);
}
