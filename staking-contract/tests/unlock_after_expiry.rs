//! User `unlock` after lock period ends.

mod common;

use common::{
    BUYER, acct, ctx_ts, deploy, one_yocto, register_buyer, setup_catalog_near_oneoff,
    unwrap_sync_lock_id,
};
use near_sdk::json_types::U64;
use near_sdk::{NearToken, testing_env};
use staking_contract::types::LockStatus;

#[test]
#[ignore = "unlock returns Promise; assert final state via sandbox until host tests resolve CC receipts"]
fn unlock_after_end_ns_requests_unlock() {
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

    let after = c.get_lock(lock_id).expect("lock");
    assert_eq!(after.status, LockStatus::UnlockRequested);
}
