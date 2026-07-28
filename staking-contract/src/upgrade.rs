use crate::*;
use near_sdk::env;
use near_sdk::store::{IterableMap, IterableSet, LookupMap, Vector};

#[cfg(target_arch = "wasm32")]
use near_sdk::{Gas, sys};

#[cfg(target_arch = "wasm32")]
const MIGRATE_STATE_GAS: Gas = Gas::from_tgas(50);
#[cfg(target_arch = "wasm32")]
const GET_CONFIG_GAS: Gas = Gas::from_tgas(5);

#[near]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state() -> Self {
        let old: ContractV1_1_1 = env::state_read().unwrap();
        old.into()
    }

    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[allow(non_camel_case_types)]
#[near(serializers = [borsh])]
// Source layout after the #119 upgrade: purchase indexes already use V2 prefixes.
struct ContractV1_1_1 {
    pub config: VConfig,
    pub paused: bool,
    pub validators: LookupMap<ValidatorId, VValidator>,
    pub validator_ids: Vector<ValidatorId>,
    pub product_ids: Vector<ProductId>,
    pub products: LookupMap<ProductId, VProduct>,
    pub prices: LookupMap<PriceId, VPrice>,
    pub accounts: LookupMap<AccountId, VAccount>,
    pub subscriptions: LookupMap<SubscriptionId, VSubscription>,
    pub locks: LookupMap<LockId, VLock>,
    pub user_validator_shares: LookupMap<(AccountId, ValidatorId), u128>,
    pub user_pending_unstake: LookupMap<(AccountId, ValidatorId), Vec<PendingUnstakeTranche>>,
    pub user_pending_unstake_validator_count: LookupMap<AccountId, u32>,
    pub user_lock_count: LookupMap<AccountId, u32>,
    pub purchases: LookupMap<PurchaseId, VPurchase>,
    pub purchase_ids: Vector<PurchaseId>,
    pub purchases_by_account: LookupMap<AccountId, Vector<PurchaseId>>,
    pub purchases_by_product: LookupMap<ProductId, Vector<PurchaseId>>,
    pub user_purchase_count: LookupMap<AccountId, u32>,
    pub revenue_by_validator: LookupMap<ValidatorId, NearToken>,
    pub farm_pools: LookupMap<PriceId, VFarmPool>,
    pub farm_positions: LookupMap<(AccountId, ProductId), VFarmPosition>,
    pub farm_position_products_by_account: LookupMap<AccountId, Vec<ProductId>>,
    pub user_farm_position_count: LookupMap<AccountId, u32>,
    pub farm_accounts: LookupMap<AccountId, VFarmAccount>,
    pub subscription_by_account_product: LookupMap<(AccountId, ProductId), SubscriptionId>,
    pub subscriptions_by_account: LookupMap<AccountId, Vec<SubscriptionId>>,
    pub subscription_ids: IterableMap<SubscriptionId, ()>,
    pub pending_update_target_price_counts: LookupMap<PriceId, u32>,
    pub pending_update_target_product_counts: LookupMap<ProductId, u32>,
    pub id_nonce: u64,
}

impl From<ContractV1_1_1> for Contract {
    fn from(mut old: ContractV1_1_1) -> Self {
        let mut account_ids = IterableSet::new(StorageKeys::AccountIds);
        let mut subscriptions_by_product = LookupMap::new(StorageKeys::SubscriptionsByProduct);
        let mut lock_ids = IterableSet::new(StorageKeys::LockIds);
        let mut locks_by_account = LookupMap::new(StorageKeys::LocksByAccount);

        for (subscription_id, _) in old.subscription_ids.iter() {
            if let Some(subscription) = old.subscriptions.get(subscription_id) {
                let mut subscription: Subscription = subscription.clone().into();
                normalize_subscription_anchor_day_for_migration(&mut subscription);
                old.subscriptions
                    .insert(subscription_id.clone(), subscription.clone().into());
                index_account_if_registered(
                    &mut account_ids,
                    &old.accounts,
                    &subscription.account_id,
                );
                add_subscription_to_product_index_for_migration(
                    &mut subscriptions_by_product,
                    &subscription.product_id,
                    subscription_id,
                );
                add_lock_to_indexes_for_migration(
                    &mut lock_ids,
                    &mut locks_by_account,
                    &old.locks,
                    &subscription.last_lock_id,
                );
            }
        }

        for purchase_id in old.purchase_ids.iter() {
            if let Some(purchase) = old.purchases.get(purchase_id) {
                let purchase: Purchase = purchase.clone().into();
                index_account_if_registered(&mut account_ids, &old.accounts, &purchase.account_id);
            }
        }

        for validator_id in old.validator_ids.iter() {
            if let Some(validator) = old.validators.get(validator_id) {
                for account_id in validator.legacy_accounts_with_pending_unstake() {
                    index_account_if_registered(&mut account_ids, &old.accounts, account_id);
                }
            }
        }

        Self {
            config: old.config,
            paused: old.paused,
            validators: old.validators,
            validator_ids: old.validator_ids,
            product_ids: old.product_ids,
            products: old.products,
            prices: old.prices,
            accounts: old.accounts,
            account_ids,
            subscriptions: old.subscriptions,
            locks: old.locks,
            lock_ids,
            locks_by_account,
            user_validator_shares: old.user_validator_shares,
            user_pending_unstake: old.user_pending_unstake,
            user_pending_unstake_validator_count: old.user_pending_unstake_validator_count,
            user_lock_count: old.user_lock_count,
            purchases: old.purchases,
            purchase_ids: old.purchase_ids,
            purchases_by_account: old.purchases_by_account,
            purchases_by_product: old.purchases_by_product,
            user_purchase_count: old.user_purchase_count,
            revenue_by_validator: old.revenue_by_validator,
            farm_pools: old.farm_pools,
            farm_positions: old.farm_positions,
            farm_position_products_by_account: old.farm_position_products_by_account,
            user_farm_position_count: old.user_farm_position_count,
            farm_accounts: old.farm_accounts,
            subscription_by_account_product: old.subscription_by_account_product,
            subscriptions_by_account: old.subscriptions_by_account,
            subscriptions_by_product,
            subscription_ids: old.subscription_ids,
            pending_update_target_price_counts: old.pending_update_target_price_counts,
            pending_update_target_product_counts: old.pending_update_target_product_counts,
            id_nonce: old.id_nonce,
        }
    }
}

fn normalize_subscription_anchor_day_for_migration(subscription: &mut Subscription) {
    subscription.anchor_day =
        crate::subscriptions::billing_anchor_day_from_timestamp(subscription.start_ns.0);
}

fn index_account_if_registered(
    account_ids: &mut IterableSet<AccountId>,
    old_accounts: &LookupMap<AccountId, VAccount>,
    account_id: &AccountId,
) {
    if old_accounts.get(account_id).is_some() {
        account_ids.insert(account_id.clone());
    }
}

fn add_subscription_to_product_index_for_migration(
    subscriptions_by_product: &mut LookupMap<ProductId, IterableSet<SubscriptionId>>,
    product_id: &ProductId,
    subscription_id: &SubscriptionId,
) {
    if let Some(ids) = subscriptions_by_product.get_mut(product_id) {
        ids.insert(subscription_id.clone());
        return;
    }

    let mut ids = IterableSet::new(Contract::subscriptions_by_product_set_key(product_id));
    ids.insert(subscription_id.clone());
    subscriptions_by_product.insert(product_id.clone(), ids);
}

fn add_lock_to_indexes_for_migration(
    lock_ids: &mut IterableSet<LockId>,
    locks_by_account: &mut LookupMap<AccountId, Vector<LockId>>,
    old_locks: &LookupMap<LockId, VLock>,
    lock_id: &LockId,
) {
    let Some(lock) = old_locks.get(lock_id) else {
        return;
    };
    if lock_ids.iter().any(|stored| stored == lock_id) {
        return;
    }
    let lock: Lock = lock.clone().into();
    lock_ids.insert(lock_id.clone());
    if let Some(ids) = locks_by_account.get_mut(&lock.account_id) {
        ids.push(lock_id.clone());
        return;
    }

    let mut ids = Vector::new(Contract::locks_by_account_vector_key(&lock.account_id));
    ids.push(lock_id.clone());
    locks_by_account.insert(lock.account_id, ids);
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn upgrade() {
    env::setup_panic_hook();
    let contract: Contract = env::state_read().unwrap();
    contract.assert_owner();
    let current_account_id = env::current_account_id();
    let current_account_id = current_account_id.as_str();
    let migrate_method_name = b"migrate_state".to_vec();
    let get_config_method_name = b"get_config".to_vec();
    let empty_args = b"{}".to_vec();
    unsafe {
        sys::input(0);
        let promise_id = sys::promise_batch_create(
            current_account_id.len() as _,
            current_account_id.as_ptr() as _,
        );
        sys::promise_batch_action_deploy_contract(promise_id, u64::MAX as _, 0);

        sys::promise_batch_action_function_call_weight(
            promise_id,
            migrate_method_name.len() as _,
            migrate_method_name.as_ptr() as _,
            empty_args.len() as _,
            empty_args.as_ptr() as _,
            0 as _,
            MIGRATE_STATE_GAS.as_gas(),
            1,
        );
        sys::promise_batch_action_function_call(
            promise_id,
            get_config_method_name.len() as _,
            get_config_method_name.as_ptr() as _,
            empty_args.len() as _,
            empty_args.as_ptr() as _,
            0 as _,
            GET_CONFIG_GAS.as_gas(),
        );
        sys::promise_return(promise_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::json_types::{U64, U128};
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{AccountId, NearToken, testing_env};

    const JUL_28_2026_063128_629578_NS: u64 = 1_785_220_288_629_578_000;
    const AUG_01_2026_000000_000000_NS: u64 = 1_785_542_400_000_000_000;
    const AUG_28_2026_063128_629578_NS: u64 = 1_787_898_688_629_578_000;

    fn acct(s: &str) -> AccountId {
        s.parse().expect("valid account id")
    }

    fn test_config() -> Config {
        Config {
            owner_account_id: acct("owner.near"),
            proposed_new_owner_account_id: None,
            guardians: vec![],
            min_lock_duration_ns: U64(1),
            max_lock_duration_ns: U64(u64::MAX / 8),
            epoch_unstake_settle_epochs: 4,
            min_storage_deposit: NearToken::from_millinear(100),
            per_lock_storage_stake: NearToken::from_near(0),
            per_farm_position_storage_stake: NearToken::from_near(0),
            per_purchase_storage_stake: NearToken::from_near(0),
            min_lock_amount: NearToken::from_near(1),
        }
    }

    fn test_context(block_timestamp_ns: u64) {
        let account_id = acct("staking.near");
        testing_env!(
            VMContextBuilder::new()
                .current_account_id(account_id.clone())
                .predecessor_account_id(account_id.clone())
                .signer_account_id(account_id)
                .block_timestamp(block_timestamp_ns)
                .build()
        );
    }

    fn contract_v1_1_1_from_current(contract: Contract) -> ContractV1_1_1 {
        let Contract {
            config,
            paused,
            validators,
            validator_ids,
            product_ids,
            products,
            prices,
            accounts,
            account_ids: _,
            subscriptions,
            locks,
            lock_ids: _,
            locks_by_account: _,
            user_validator_shares,
            user_pending_unstake,
            user_pending_unstake_validator_count,
            user_lock_count,
            purchases,
            purchase_ids,
            purchases_by_account,
            purchases_by_product,
            user_purchase_count,
            revenue_by_validator,
            farm_pools,
            farm_positions,
            farm_position_products_by_account,
            user_farm_position_count,
            farm_accounts,
            subscription_by_account_product,
            subscriptions_by_account,
            subscriptions_by_product: _,
            subscription_ids,
            pending_update_target_price_counts,
            pending_update_target_product_counts,
            id_nonce,
        } = contract;

        ContractV1_1_1 {
            config,
            paused,
            validators,
            validator_ids,
            product_ids,
            products,
            prices,
            accounts,
            subscriptions,
            locks,
            user_validator_shares,
            user_pending_unstake,
            user_pending_unstake_validator_count,
            user_lock_count,
            purchases,
            purchase_ids,
            purchases_by_account,
            purchases_by_product,
            user_purchase_count,
            revenue_by_validator,
            farm_pools,
            farm_positions,
            farm_position_products_by_account,
            user_farm_position_count,
            farm_accounts,
            subscription_by_account_product,
            subscriptions_by_account,
            subscription_ids,
            pending_update_target_price_counts,
            pending_update_target_product_counts,
            id_nonce,
        }
    }

    #[test]
    fn migration_recomputes_legacy_subscription_anchor_day() {
        test_context(AUG_01_2026_000000_000000_NS);
        let mut current = Contract::new(test_config());
        let subscription_id = "sub_legacy".to_string();
        let lock_id = "lock_legacy".to_string();
        let buyer = acct("buyer.near");
        current.accounts.insert(
            buyer.clone(),
            Account {
                storage_deposit: NearToken::from_near(1),
            }
            .into(),
        );
        current.locks.insert(
            lock_id.clone(),
            Lock {
                lock_id: lock_id.clone(),
                account_id: buyer.clone(),
                validator_id: acct("pool.near"),
                amount_near: NearToken::from_near(5),
                shares: U128(5),
                start_ns: U64(JUL_28_2026_063128_629578_NS),
                end_ns: U64(AUG_28_2026_063128_629578_NS),
                order: OrderRef::Subscription {
                    subscription_id: subscription_id.clone(),
                    price_id: "price_legacy".to_string(),
                    period_start_ns: U64(JUL_28_2026_063128_629578_NS),
                    period_end_ns: U64(AUG_28_2026_063128_629578_NS),
                },
                status: LockStatus::Active,
            }
            .into(),
        );
        let subscription = Subscription {
            subscription_id: subscription_id.clone(),
            account_id: buyer.clone(),
            product_id: "prod_legacy".to_string(),
            price_id: "price_legacy".to_string(),
            start_ns: U64(JUL_28_2026_063128_629578_NS),
            end_ns: U64(AUG_28_2026_063128_629578_NS),
            anchor_day: 17,
            last_lock_id: lock_id.clone(),
            status: SubscriptionStatus::Active,
            cancel_at_period_end: false,
            pending_update: None,
        };
        current
            .subscriptions
            .insert(subscription_id.clone(), subscription.into());
        current.subscription_ids.insert(subscription_id.clone(), ());

        let old = contract_v1_1_1_from_current(current);
        let migrated: Contract = old.into();
        let stored: Subscription = migrated
            .subscriptions
            .get(&subscription_id)
            .expect("subscription migrated")
            .clone()
            .into();

        assert_eq!(stored.anchor_day, 28);
        assert_eq!(
            migrated.project_subscription_view_now(stored).end_ns.0,
            AUG_28_2026_063128_629578_NS
        );
        assert_eq!(migrated.get_locks(0, 10, None)[0].lock_id, lock_id);
        assert_eq!(
            migrated.get_locks_for_account(buyer, 0, 10, None)[0].lock_id,
            lock_id
        );
    }
}
