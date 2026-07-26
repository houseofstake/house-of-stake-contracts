mod common;

use common::{
    BUYER, POOL, STAKING, VALIDATOR_OWNER_ACCOUNT, acct, add_validator_allowlisted, base_config,
    ctx, ctx_ts, deploy, deploy_with_config, one_yocto, register_buyer, setup_catalog_farm,
    setup_catalog_near_oneoff, testing_env_catalog_callback, testing_env_catalog_callback_at,
};
use near_sdk::json_types::{U64, U128};
use near_sdk::{NearToken, PromiseOrValue, testing_env};
use staking_contract::stake::{
    FARM_REWARD_RATE_DENOM, NS_PER_SECOND, YOCTO_PER_NEAR, farm_shares_for_amount_ceil,
};
use staking_contract::types::{FarmPosition, FarmStatus, PriceMetadata, PriceType};

const BASE_TS: u64 = 1_700_000_000_000_000_000;
const SIX_DAYS_NS: u64 = 6 * 86_400 * NS_PER_SECOND as u64;
const SPEC_REWARD_RATE: u128 = 3_858_024_691_358_024;

fn unwrap_sync_position(result: PromiseOrValue<FarmPosition>) -> FarmPosition {
    match result {
        PromiseOrValue::Value(position) => position,
        PromiseOrValue::Promise(_) => panic!("unit tests expect synchronous farm stake"),
    }
}

#[test]
fn farm_stake_accrues_rewards_in_account_view() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(100), BASE_TS));
    let position = unwrap_sync_position(c.stake(product_id.clone(), None));
    assert_eq!(position.status, FarmStatus::Active);

    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_yoctonear(0),
        BASE_TS + SIX_DAYS_NS
    ));
    let account = c.get_farm_account(acct(BUYER));
    assert_eq!(
        account.pending_reward_units.0,
        199_999_999_999_999_964_160_000
    );
    assert_eq!(
        account.total_earned_reward_units.0,
        199_999_999_999_999_964_160_000
    );
    assert_eq!(account.active_positions.len(), 1);
    assert_eq!(
        account.active_positions[0].staked_near_amount.0,
        NearToken::from_near(100).as_yoctonear()
    );
    assert_eq!(
        account.active_positions[0].pending_reward_units.0,
        account.pending_reward_units.0
    );
    assert_eq!(
        account.active_positions[0].total_earned_reward_units.0,
        account.pending_reward_units.0
    );

    let position_view = c
        .get_farm_position(acct(BUYER), product_id)
        .expect("position view");
    assert_eq!(
        position_view.staked_near_amount.0,
        NearToken::from_near(100).as_yoctonear()
    );
    assert_eq!(
        position_view.pending_reward_units.0,
        199_999_999_999_999_964_160_000
    );
    assert_eq!(
        position_view.total_earned_reward_units.0,
        position_view.pending_reward_units.0
    );
}

#[test]
#[should_panic(expected = "Farm stake exceeds max_amount")]
fn farm_stake_rejects_amount_above_max_amount() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        Some(NearToken::from_near(5).as_yoctonear()),
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(6), BASE_TS));
    let _ = c.stake(product_id, None);
}

#[test]
#[should_panic(expected = "Use stake for farm prices")]
fn lock_rejects_farm_price() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(10)));
    let _ = c.lock(Some(price_id), None, Some(U64(1)));
}

#[test]
#[should_panic(expected = "Product already has an active farm price")]
fn create_farm_price_rejects_second_active_farm_price_for_product() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        "Second farm".into(),
        "".into(),
        U128(NearToken::from_near(1).as_yoctonear()),
        PriceType::Farm,
        None,
        U128(0),
        Some(PriceMetadata {
            max_amount: None,
            farm_reward_rate: Some(U128(SPEC_REWARD_RATE)),
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "farm_reward_rate is only valid for Farm prices")]
fn edit_non_farm_price_rejects_farm_reward_rate() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_near_oneoff(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id,
        None,
        None,
        Some(PriceMetadata {
            max_amount: None,
            farm_reward_rate: Some(U128(SPEC_REWARD_RATE)),
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
#[should_panic(expected = "Farm price requires farm_reward_rate")]
fn create_farm_price_requires_reward_rate() {
    let mut c = deploy();
    add_validator_allowlisted(&mut c);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    let product_id = c.create_product_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        acct(POOL),
        "Farm product".into(),
        "Desc".into(),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.create_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        product_id,
        "Bad farm".into(),
        "".into(),
        U128(NearToken::from_near(1).as_yoctonear()),
        PriceType::Farm,
        None,
        U128(0),
        Some(PriceMetadata {
            max_amount: None,
            farm_reward_rate: None,
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
fn farm_stake_twice_aggregates_one_position() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(5), BASE_TS));
    let first = unwrap_sync_position(c.stake(product_id.clone(), None));
    testing_env!(ctx_ts(
        acct(BUYER),
        NearToken::from_near(7),
        BASE_TS + NS_PER_SECOND as u64
    ));
    let second = unwrap_sync_position(c.stake(product_id.clone(), None));

    assert!(second.shares.0 > first.shares.0);
    let positions = c.get_farm_positions_for_account(acct(BUYER), 0, 10);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].shares.0, second.shares.0);
    assert_eq!(c.get_price(price_id).expect("price").usage_count, 2);
    assert_eq!(c.get_product(product_id).expect("product").usage_count, 2);
}

#[test]
fn partial_unstake_keeps_position_active_and_preserves_unclaimed_rewards() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(10), BASE_TS));
    let position = unwrap_sync_position(c.stake(product_id.clone(), None));
    let shares_remove = position.shares.0 / 2;

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), BASE_TS + SIX_DAYS_NS));
    let _ = c.unstake(
        product_id.clone(),
        Some(U128(NearToken::from_near(5).as_yoctonear())),
    );
    testing_env!(ctx_ts(
        acct(STAKING),
        NearToken::from_yoctonear(0),
        BASE_TS + SIX_DAYS_NS
    ));
    c.resolve_farm_unstake(
        acct(BUYER),
        product_id.clone(),
        acct(POOL),
        Some(U128(NearToken::from_near(5).as_yoctonear())),
    );

    let remaining = c
        .get_farm_position(acct(BUYER), product_id.clone())
        .expect("position");
    assert_eq!(remaining.status, FarmStatus::Active);
    assert_eq!(remaining.shares.0, position.shares.0 - shares_remove);
    assert!(
        remaining.accrued_reward_units.0 > 0,
        "partial unstake should retain settled rewards on the active position"
    );
    assert!(remaining.staked_near_amount.0 > 0);
    assert!(remaining.pending_reward_units.0 >= remaining.accrued_reward_units.0);
    assert_eq!(
        remaining.total_earned_reward_units.0,
        remaining.pending_reward_units.0
    );
    assert_eq!(
        c.get_farm_account(acct(BUYER)).accumulated_reward_units.0,
        0
    );
    assert!(c.get_farm_account(acct(BUYER)).pending_reward_units.0 > 0);
}

#[test]
fn partial_unstake_prices_amount_after_settlement() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(10), BASE_TS));
    let position = unwrap_sync_position(c.stake(product_id.clone(), None));

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), BASE_TS + SIX_DAYS_NS));
    let _ = c.unstake(
        product_id.clone(),
        Some(U128(NearToken::from_near(5).as_yoctonear())),
    );

    let mut validator = c.get_validator(acct(POOL)).expect("validator");
    validator.total_staked_balance = NearToken::from_near(20);
    let expected_shares_remove = farm_shares_for_amount_ceil(
        NearToken::from_near(5).as_yoctonear(),
        validator.total_shares.0,
        validator.net_stake_yocto(),
        position.shares.0,
    );
    c.validators.insert(acct(POOL), validator.into());

    testing_env!(ctx_ts(
        acct(STAKING),
        NearToken::from_yoctonear(0),
        BASE_TS + SIX_DAYS_NS
    ));
    c.resolve_farm_unstake(
        acct(BUYER),
        product_id.clone(),
        acct(POOL),
        Some(U128(NearToken::from_near(5).as_yoctonear())),
    );

    let remaining = c
        .get_farm_position(acct(BUYER), product_id)
        .expect("position");
    assert_eq!(remaining.status, FarmStatus::Active);
    assert_eq!(
        remaining.shares.0,
        position.shares.0 - expected_shares_remove
    );
    assert!(
        expected_shares_remove < NearToken::from_near(5).as_yoctonear(),
        "post-settlement pricing should not use the stale pre-settlement 1:1 share price"
    );
}

#[test]
fn full_unstake_closes_position_and_rolls_rewards_into_account() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(100), BASE_TS));
    let _position = unwrap_sync_position(c.stake(product_id.clone(), None));

    testing_env!(ctx_ts(acct(BUYER), one_yocto(), BASE_TS + SIX_DAYS_NS));
    let _ = c.unstake(product_id.clone(), None);
    testing_env!(ctx_ts(
        acct(STAKING),
        NearToken::from_yoctonear(0),
        BASE_TS + SIX_DAYS_NS
    ));
    c.resolve_farm_unstake(acct(BUYER), product_id.clone(), acct(POOL), None);

    let closed = c
        .get_farm_position(acct(BUYER), product_id)
        .expect("position");
    assert_eq!(closed.status, FarmStatus::Closed);
    assert_eq!(closed.shares.0, 0);
    assert_eq!(closed.staked_near_amount.0, 0);
    assert_eq!(closed.pending_reward_units.0, 0);
    assert_eq!(closed.total_earned_reward_units.0, 0);
    let account = c.get_farm_account(acct(BUYER));
    assert_eq!(
        account.accumulated_reward_units.0,
        199_999_999_999_999_964_160_000
    );
    assert_eq!(account.pending_reward_units.0, 0);
    assert_eq!(
        account.total_earned_reward_units.0,
        199_999_999_999_999_964_160_000
    );
}

#[test]
fn archive_farm_price_succeeds_after_full_unstake() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(10), BASE_TS));
    let _position = unwrap_sync_position(c.stake(product_id.clone(), None));
    testing_env!(ctx_ts(acct(BUYER), one_yocto(), BASE_TS + SIX_DAYS_NS));
    let _ = c.unstake(product_id.clone(), None);
    testing_env!(ctx_ts(
        acct(STAKING),
        NearToken::from_yoctonear(0),
        BASE_TS + SIX_DAYS_NS
    ));
    c.resolve_farm_unstake(acct(BUYER), product_id, acct(POOL), None);

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
fn edit_farm_reward_rate_settles_old_rate_first() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx_ts(acct(BUYER), NearToken::from_near(100), BASE_TS));
    let _ = unwrap_sync_position(c.stake(product_id, None));

    testing_env_catalog_callback_at(acct(VALIDATOR_OWNER_ACCOUNT), BASE_TS + SIX_DAYS_NS);
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        None,
        None,
        Some(PriceMetadata {
            max_amount: None,
            farm_reward_rate: Some(U128(SPEC_REWARD_RATE * 2)),
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let pool = c.get_farm_pool(price_id).expect("farm pool");
    assert!(pool.acc_reward_per_share.0 > 0);
    assert_eq!(pool.reward_rate.0, SPEC_REWARD_RATE * 2);
}

#[test]
fn edit_farm_metadata_preserves_reward_rate_when_omitted() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        None,
        None,
        Some(PriceMetadata {
            max_amount: Some(U128(NearToken::from_near(50).as_yoctonear())),
            farm_reward_rate: None,
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );

    let price = c.get_price(price_id.clone()).expect("price");
    let metadata = price.metadata.expect("metadata");
    assert_eq!(
        metadata.max_amount,
        Some(U128(NearToken::from_near(50).as_yoctonear()))
    );
    assert_eq!(metadata.farm_reward_rate, Some(U128(SPEC_REWARD_RATE)));
    assert_eq!(
        c.get_farm_pool(price_id).expect("farm pool").reward_rate,
        U128(SPEC_REWARD_RATE)
    );
}

#[test]
fn edit_farm_metadata_sequence_keeps_price_and_pool_rates_consistent() {
    let mut c = deploy();
    let (_product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );

    testing_env_catalog_callback_at(
        acct(VALIDATOR_OWNER_ACCOUNT),
        BASE_TS + NS_PER_SECOND as u64,
    );
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        None,
        None,
        Some(PriceMetadata {
            max_amount: Some(U128(NearToken::from_near(50).as_yoctonear())),
            farm_reward_rate: None,
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    assert_farm_price_and_pool_metadata(
        &c,
        &price_id,
        Some(U128(NearToken::from_near(50).as_yoctonear())),
        U128(SPEC_REWARD_RATE),
    );

    testing_env_catalog_callback_at(
        acct(VALIDATOR_OWNER_ACCOUNT),
        BASE_TS + 2 * NS_PER_SECOND as u64,
    );
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        None,
        None,
        Some(PriceMetadata {
            max_amount: None,
            farm_reward_rate: Some(U128(SPEC_REWARD_RATE * 2)),
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    assert_farm_price_and_pool_metadata(&c, &price_id, None, U128(SPEC_REWARD_RATE * 2));

    testing_env_catalog_callback_at(
        acct(VALIDATOR_OWNER_ACCOUNT),
        BASE_TS + 3 * NS_PER_SECOND as u64,
    );
    c.edit_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id.clone(),
        None,
        None,
        Some(PriceMetadata {
            max_amount: Some(U128(NearToken::from_near(25).as_yoctonear())),
            farm_reward_rate: None,
        }),
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
    assert_farm_price_and_pool_metadata(
        &c,
        &price_id,
        Some(U128(NearToken::from_near(25).as_yoctonear())),
        U128(SPEC_REWARD_RATE * 2),
    );
}

fn assert_farm_price_and_pool_metadata(
    c: &staking_contract::Contract,
    price_id: &String,
    expected_max_amount: Option<U128>,
    expected_reward_rate: U128,
) {
    let price = c.get_price(price_id.clone()).expect("price");
    let metadata = price.metadata.expect("metadata");
    assert_eq!(metadata.max_amount, expected_max_amount);
    assert_eq!(metadata.farm_reward_rate, Some(expected_reward_rate));
    assert_eq!(
        c.get_farm_pool(price_id.clone())
            .expect("farm pool")
            .reward_rate,
        expected_reward_rate
    );
}

#[test]
fn storage_unregister_fails_with_active_farm_position() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(1)));
    let _ = unwrap_sync_position(c.stake(product_id, None));
    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));
}

#[test]
#[should_panic(expected = "Top up storage for another farm position")]
fn farm_stake_requires_storage_for_new_position_record() {
    let mut cfg = base_config();
    cfg.per_farm_position_storage_stake = NearToken::from_near(1);
    let mut c = deploy_with_config(cfg);
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(1)));
    let _ = c.stake(product_id, None);
}

#[test]
fn farm_position_storage_count_is_charged_once_per_product_position() {
    let mut cfg = base_config();
    cfg.per_lock_storage_stake = NearToken::from_near(100);
    cfg.per_farm_position_storage_stake = NearToken::from_near(1);
    let mut c = deploy_with_config(cfg);
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );

    testing_env!(ctx(acct(BUYER), NearToken::from_near(2)));
    c.storage_deposit(None, None);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(2)));
    let _ = unwrap_sync_position(c.stake(product_id.clone(), None));
    assert_eq!(
        c.user_farm_position_count.get(&acct(BUYER)).copied(),
        Some(1)
    );

    testing_env!(ctx(acct(BUYER), one_yocto()));
    let _ = c.unstake(product_id.clone(), None);
    testing_env!(ctx(acct(STAKING), NearToken::from_yoctonear(0)));
    c.resolve_farm_unstake(acct(BUYER), product_id.clone(), acct(POOL), None);
    c.on_epoch_pipeline_terminal_release(acct(POOL));

    testing_env!(ctx(acct(BUYER), NearToken::from_near(2)));
    let _ = unwrap_sync_position(c.stake(product_id, None));
    assert_eq!(
        c.user_farm_position_count.get(&acct(BUYER)).copied(),
        Some(1)
    );
}

#[test]
fn farm_active_position_count_tracks_close_and_reopen() {
    let mut c = deploy();
    let (product_id, _price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(10)));
    let _ = unwrap_sync_position(c.stake(product_id.clone(), None));
    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));

    let _ = c.unstake(
        product_id.clone(),
        Some(U128(NearToken::from_near(4).as_yoctonear())),
    );
    testing_env!(ctx(acct(STAKING), NearToken::from_yoctonear(0)));
    c.resolve_farm_unstake(
        acct(BUYER),
        product_id.clone(),
        acct(POOL),
        Some(U128(NearToken::from_near(4).as_yoctonear())),
    );
    c.on_epoch_pipeline_terminal_release(acct(POOL));
    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));

    testing_env!(ctx(acct(BUYER), one_yocto()));
    let _ = c.unstake(product_id.clone(), None);
    testing_env!(ctx(acct(STAKING), NearToken::from_yoctonear(0)));
    c.resolve_farm_unstake(acct(BUYER), product_id.clone(), acct(POOL), None);
    c.on_epoch_pipeline_terminal_release(acct(POOL));

    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));

    testing_env!(ctx(acct(BUYER), NearToken::from_near(3)));
    let reopened = unwrap_sync_position(c.stake(product_id.clone(), None));
    assert_eq!(reopened.status, FarmStatus::Active);
    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));

    testing_env!(ctx(acct(BUYER), one_yocto()));
    let _ = c.unstake(product_id.clone(), None);
    testing_env!(ctx(acct(STAKING), NearToken::from_yoctonear(0)));
    c.resolve_farm_unstake(acct(BUYER), product_id, acct(POOL), None);
    testing_env!(ctx(acct(BUYER), one_yocto()));
    assert!(!c.storage_unregister(None));
}

#[test]
#[should_panic(expected = "Cannot archive a farm price while it has active stake")]
fn archive_farm_price_fails_with_active_stake() {
    let mut c = deploy();
    let (product_id, price_id) = setup_catalog_farm(
        &mut c,
        SPEC_REWARD_RATE,
        NearToken::from_near(1).as_yoctonear(),
        None,
    );
    register_buyer(&mut c);

    testing_env!(ctx(acct(BUYER), NearToken::from_near(1)));
    let _ = unwrap_sync_position(c.stake(product_id, None));

    testing_env_catalog_callback(acct(VALIDATOR_OWNER_ACCOUNT));
    c.archive_price_after_get_owner(
        acct(VALIDATOR_OWNER_ACCOUNT),
        price_id,
        acct(VALIDATOR_OWNER_ACCOUNT),
    );
}

#[test]
fn farm_reward_rate_denominator_is_stable() {
    assert_eq!(FARM_REWARD_RATE_DENOM, 1);
    assert_eq!(YOCTO_PER_NEAR, NearToken::from_near(1).as_yoctonear());
}
