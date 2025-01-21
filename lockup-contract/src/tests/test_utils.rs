use std::str::FromStr;
use near_sdk::{AccountId, Gas, NearToken, PublicKey, VMContext};

pub const LOCKUP_NEAR: u128 = 1000;
pub const GENESIS_TIME_IN_DAYS: u64 = 500;
pub const YEAR: u64 = 365;

pub fn lockup_account() -> AccountId {
    AccountId::from_str("lockup").unwrap()
}

pub fn system_account() -> AccountId {
    AccountId::from_str("system").unwrap()
}

pub fn account_owner() -> AccountId {
    AccountId::from_str("account_owner").unwrap()
}

pub fn non_owner() -> AccountId {
    AccountId::from_str("non_owner").unwrap()
}

pub fn to_yocto(near_balance: u128) -> u128 {
    near_balance * 10u128.pow(24)
}

pub fn to_nanos(num_days: u64) -> u64 {
    num_days * 86400_000_000_000
}

pub fn to_ts(num_days: u64) -> u64 {
    // 2018-08-01 UTC in nanoseconds
    1533081600_000_000_000 + to_nanos(num_days)
}

pub fn assert_almost_eq_with_max_delta(left: u128, right: u128, max_delta: u128) {
    assert!(
        std::cmp::max(left, right) - std::cmp::min(left, right) < max_delta,
        "{}",
        format!(
            "Left {} is not even close to Right {} within delta {}",
            left, right, max_delta
        )
    );
}

pub fn assert_almost_eq(left: u128, right: u128) {
    assert_almost_eq_with_max_delta(left, right, to_yocto(10));
}

pub fn get_context(
    predecessor_account_id: AccountId,
    account_balance: u128,
    account_locked_balance: u128,
    block_timestamp: u64,
) -> VMContext {
    VMContext {
        current_account_id: lockup_account(),
        signer_account_id: predecessor_account_id.clone(),
        signer_account_pk: public_key(123),
        predecessor_account_id,
        input: vec![],
        block_index: 1,
        block_timestamp,
        epoch_height: 1,
        account_balance: NearToken::from_yoctonear(account_balance),
        account_locked_balance: NearToken::from_yoctonear(account_locked_balance),
        storage_usage: 10u64.pow(6),
        attached_deposit: NearToken::from_yoctonear(0),
        prepaid_gas: Gas::from_gas(10u64.pow(15)),
        random_seed: [37u8; 32],
        output_data_receivers: vec![],
        view_config: None,
    }
}

// version based on Base58PublicKey, before the contract upgrade
// pub fn public_key(byte_val: u8) -> Base58PublicKey {
//     let mut pk = vec![byte_val; 33];
//     pk[0] = 0;
//     Base58PublicKey(pk)
// }

pub fn public_key(byte_val: u8) -> PublicKey {
    let mut pk = vec![byte_val; 33];
    pk[0] = 0;
    PublicKey::try_from(pk).unwrap()
}

