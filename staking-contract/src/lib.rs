#![allow(clippy::too_many_arguments)]

pub mod accounts;
pub mod config;
pub mod epoch;
pub mod events;
pub mod gas;
pub mod governance;
pub mod ids;
pub mod lock;
pub mod pause;
pub mod payments;
pub mod prices;
pub mod products;
pub mod stake;
pub mod subscriptions;
pub mod types;
pub mod unlock;
pub mod upgrade;
pub mod utils;
pub mod validators;
pub mod withdraw;

pub use config::Config;
pub use types::*;

use near_sdk::store::{IterableMap, LookupMap, Vector};
use near_sdk::{AccountId, BorshStorageKey, NearToken, PanicOnDefault, near};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Validators,
    ValidatorIds,
    ProductIds,
    Products,
    Prices,
    Accounts,
    Subscriptions,
    Locks,
    UserValidatorShares,
    UserPendingUnstake,
    UserLockCount,
    SubscriptionByAccountProduct,
    SubscriptionsByAccount,
    SubscriptionIds,
    PendingUpdateTargetPriceCounts,
    PendingUpdateTargetProductCounts,
    Purchases,
    PurchaseIds,
    PurchasesByAccount,
    PurchasesByProduct,
    UserPurchaseCount,
    RevenueByValidator,
    FarmPools,
    FarmPositions,
    FarmPositionProductsByAccount,
    FarmAccounts,
    UserFarmPositionCount,
    UserPendingUnstakeValidatorCount,
    PurchasesByAccountVector { account_hash: Vec<u8> },
    PurchasesByProductVector { product_hash: Vec<u8> },
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    /// Protocol configuration: owner, guardians, pause-independent bounds (`min_lock_amount`,
    /// lock duration range), epoch settle epochs, and storage economics.
    pub config: VConfig,
    /// When `true`, user-facing mutating methods reject until [`crate::pause::Contract::unpause`].
    pub paused: bool,
    /// Allowlisted staking pools (`validator_id` = pool account). Holds share-pool and epoch pipeline state
    /// per [`Validator`].
    pub validators: LookupMap<ValidatorId, VValidator>,
    /// Creation order of allowlisted pools; drives paginated [`crate::validators::Contract::get_validators`].
    pub validator_ids: Vector<ValidatorId>,
    /// Creation order of catalog products; stable ordering for [`crate::products::Contract::get_products`].
    pub product_ids: Vector<ProductId>,
    /// Products keyed by id (`prod_*`); validator-scoped via [`Product::validator_id`](crate::types::Product::validator_id).
    pub products: LookupMap<ProductId, VProduct>,
    /// Price lines (`price_*` ids); [`Price::product_id`](crate::types::Price::product_id) links to a product.
    pub prices: LookupMap<PriceId, VPrice>,
    /// Per-user accounting: NEP-145-style registered storage (`storage_deposit`).
    pub accounts: LookupMap<AccountId, VAccount>,
    /// Subscription records keyed by [`Subscription::subscription_id`] (`sub_*`).
    pub subscriptions: LookupMap<SubscriptionId, VSubscription>,
    /// Active and historical locks keyed by [`Lock::lock_id`] (`lock_*`).
    pub locks: LookupMap<LockId, VLock>,
    /// User stake position on a pool: `(AccountId, ValidatorId)` → outstanding share units (integer, same scale as [`Validator::total_shares`]). [`ValidatorId`](crate::types::ValidatorId) is the pool contract account.
    pub user_validator_shares: LookupMap<(AccountId, ValidatorId), u128>,
    /// After unlock, NEAR liability slices for this user on this pool until [`crate::Contract::withdraw`]
    /// (epoch-gated; paid from `pending_to_claim` once pool funds are in the contract claim bucket).
    pub user_pending_unstake: LookupMap<(AccountId, ValidatorId), Vec<PendingUnstakeTranche>>,
    /// Number of validator pools for which an account has at least one pending unstake tranche.
    /// This keeps `storage_unregister` bounded by account state instead of scanning every validator.
    pub user_pending_unstake_validator_count: LookupMap<AccountId, u32>,
    /// Monotonic count of locks created per account; multiplied by [`Config::per_lock_storage_stake`] for prepaid lock storage.
    pub user_lock_count: LookupMap<AccountId, u32>,
    /// Direct one-off payment records keyed by [`Purchase::purchase_id`] (`pay_*`).
    pub purchases: LookupMap<PurchaseId, VPurchase>,
    /// Creation order of direct payment records; drives paginated purchase views.
    pub purchase_ids: Vector<PurchaseId>,
    /// Secondary index: purchaser account -> paginated purchase ids.
    pub purchases_by_account: LookupMap<AccountId, Vector<PurchaseId>>,
    /// Secondary index: product id -> paginated purchase ids.
    pub purchases_by_product: LookupMap<ProductId, Vector<PurchaseId>>,
    /// Monotonic count of direct purchases created per account; multiplied by [`Config::per_purchase_storage_stake`].
    pub user_purchase_count: LookupMap<AccountId, u32>,
    /// Withdrawable direct-payment revenue aggregated by validator pool account.
    pub revenue_by_validator: LookupMap<ValidatorId, NearToken>,
    /// Farm reward accumulator per Farm price id.
    pub farm_pools: LookupMap<PriceId, VFarmPool>,
    /// One farm position per `(account_id, product_id)`.
    pub farm_positions: LookupMap<(AccountId, ProductId), VFarmPosition>,
    /// Secondary index: farm owner account -> product ids with current or historical farm positions.
    pub farm_position_products_by_account: LookupMap<AccountId, Vec<ProductId>>,
    /// Monotonic count of farm positions created per account; multiplied by [`Config::per_farm_position_storage_stake`] for prepaid retained-position storage.
    pub user_farm_position_count: LookupMap<AccountId, u32>,
    /// Per-account rolled-up farm reward totals from closed positions.
    pub farm_accounts: LookupMap<AccountId, VFarmAccount>,
    /// Secondary index: `(subscriber, product_id)` → `subscription_id` for at-most-one subscription per product per account.
    pub subscription_by_account_product: LookupMap<(AccountId, ProductId), SubscriptionId>,
    /// Secondary index: `subscriber` → owned subscription ids. Used for account-level listing and
    /// subscription-specific plan changes without scanning the full catalog.
    pub subscriptions_by_account: LookupMap<AccountId, Vec<SubscriptionId>>,
    /// Subscription ids keyed for efficient membership and removal while remaining iterable for views.
    pub subscription_ids: IterableMap<SubscriptionId, ()>,
    /// Pending subscription-update target price reference counts, used by bounded catalog guards.
    pub pending_update_target_price_counts: LookupMap<PriceId, u32>,
    /// Pending subscription-update target product reference counts, used by bounded catalog guards.
    pub pending_update_target_product_counts: LookupMap<ProductId, u32>,
    /// Counter mixed into deterministic ids ([`crate::ids`]) for products, prices, subscriptions, locks.
    pub id_nonce: u64,
}

#[near]
impl Contract {
    #[init]
    pub fn new(config: Config) -> Self {
        crate::config::require_min_lock_amount_at_protocol_floor(&config.min_lock_amount);
        Self {
            config: config.into(),
            paused: false,
            validators: LookupMap::new(StorageKeys::Validators),
            validator_ids: Vector::new(StorageKeys::ValidatorIds),
            product_ids: Vector::new(StorageKeys::ProductIds),
            products: LookupMap::new(StorageKeys::Products),
            prices: LookupMap::new(StorageKeys::Prices),
            accounts: LookupMap::new(StorageKeys::Accounts),
            subscriptions: LookupMap::new(StorageKeys::Subscriptions),
            locks: LookupMap::new(StorageKeys::Locks),
            user_validator_shares: LookupMap::new(StorageKeys::UserValidatorShares),
            user_pending_unstake: LookupMap::new(StorageKeys::UserPendingUnstake),
            user_pending_unstake_validator_count: LookupMap::new(
                StorageKeys::UserPendingUnstakeValidatorCount,
            ),
            user_lock_count: LookupMap::new(StorageKeys::UserLockCount),
            purchases: LookupMap::new(StorageKeys::Purchases),
            purchase_ids: Vector::new(StorageKeys::PurchaseIds),
            purchases_by_account: LookupMap::new(StorageKeys::PurchasesByAccount),
            purchases_by_product: LookupMap::new(StorageKeys::PurchasesByProduct),
            user_purchase_count: LookupMap::new(StorageKeys::UserPurchaseCount),
            revenue_by_validator: LookupMap::new(StorageKeys::RevenueByValidator),
            farm_pools: LookupMap::new(StorageKeys::FarmPools),
            farm_positions: LookupMap::new(StorageKeys::FarmPositions),
            farm_position_products_by_account: LookupMap::new(
                StorageKeys::FarmPositionProductsByAccount,
            ),
            user_farm_position_count: LookupMap::new(StorageKeys::UserFarmPositionCount),
            farm_accounts: LookupMap::new(StorageKeys::FarmAccounts),
            subscription_by_account_product: LookupMap::new(
                StorageKeys::SubscriptionByAccountProduct,
            ),
            subscriptions_by_account: LookupMap::new(StorageKeys::SubscriptionsByAccount),
            subscription_ids: IterableMap::new(StorageKeys::SubscriptionIds),
            pending_update_target_price_counts: LookupMap::new(
                StorageKeys::PendingUpdateTargetPriceCounts,
            ),
            pending_update_target_product_counts: LookupMap::new(
                StorageKeys::PendingUpdateTargetProductCounts,
            ),
            id_nonce: 0,
        }
    }
}
