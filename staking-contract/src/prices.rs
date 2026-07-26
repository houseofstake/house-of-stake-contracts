//! Catalog prices: CRUD + archive lifecycle for product tiers.
//! Mutating RPCs are gated via `get_owner_id` plus validator owner-or-catalog-manager authorization.
//! Archiving/deleting a default tier clears [`Product::default_price_id`] through product helpers.

use crate::gas::callbacks;
use crate::*;
use near_sdk::ext_contract;
use near_sdk::json_types::{U64, U128};
use near_sdk::{AccountId, Promise, env, near, require};

/// Retry id generation when a collision exists in [`Contract::prices`] (extremely unlikely).
fn next_unique_price_id(contract: &mut Contract) -> PriceId {
    crate::ids::next_unique_generated_id(
        &mut contract.id_nonce,
        crate::ids::next_price_id,
        |id| contract.prices.contains_key(id),
        "Could not allocate a unique price id; try again",
    )
}

/// Self callbacks for **price** catalog after `get_owner_id` on the staking pool.
#[ext_contract(ext_self_prices)]
pub trait ExtSelfPrices {
    fn create_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        name: String,
        description: String,
        amount: U128,
        price_type: PriceType,
        billing_period: Option<BillingPeriod>,
        lock_factor_near_months: U128,
        metadata: Option<PriceMetadata>,
        expected_caller: AccountId,
    ) -> PriceId;
    fn edit_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        name: Option<String>,
        description: Option<String>,
        metadata: Option<PriceMetadata>,
        expected_caller: AccountId,
    );
    fn archive_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    );
    fn delete_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    );
    fn unarchive_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    );
}

#[near]
impl Contract {
    // -------------------------------------------------------------------------
    // Public catalog admin (pool-owner auth via promise chain)
    // -------------------------------------------------------------------------

    /// Add a price tier under an active product. Validator owner or catalog manager; attach 1 yocto.
    #[payable]
    pub fn create_price(
        &mut self,
        product_id: ProductId,
        name: String,
        description: String,
        amount: U128,
        price_type: PriceType,
        billing_period: Option<BillingPeriod>,
        lock_factor_near_months: U128,
        metadata: Option<PriceMetadata>,
    ) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_prices::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .create_price_after_get_owner(
                    product_id,
                    name,
                    description,
                    amount,
                    price_type,
                    billing_period,
                    lock_factor_near_months,
                    metadata,
                    expected_caller,
                )
        })
    }

    /// Update display fields and mutable metadata. Farm reward-rate changes are applied prospectively
    /// by settling the farm pool before the rate is changed.
    #[payable]
    pub fn edit_price(
        &mut self,
        price_id: PriceId,
        name: Option<String>,
        description: Option<String>,
        metadata: Option<PriceMetadata>,
    ) -> Promise {
        self.promise_catalog_admin_on_price(price_id, |expected_caller, price_id| {
            ext_self_prices::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .edit_price_after_get_owner(price_id, name, description, metadata, expected_caller)
        })
    }

    /// Archive: existing locks/subscriptions are unchanged; new locks must pick an active price.
    #[payable]
    pub fn archive_price(&mut self, price_id: PriceId) -> Promise {
        self.promise_catalog_admin_on_price(price_id, |expected_caller, price_id| {
            ext_self_prices::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .archive_price_after_get_owner(price_id, expected_caller)
        })
    }

    /// Restore an archived price so it can be locked or set as product default again.
    #[payable]
    pub fn unarchive_price(&mut self, price_id: PriceId) -> Promise {
        self.promise_catalog_admin_on_price(price_id, |expected_caller, price_id| {
            ext_self_prices::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .unarchive_price_after_get_owner(price_id, expected_caller)
        })
    }

    /// Delete from storage when never locked (`usage_count == 0`); also drops the id from the parent product list.
    #[payable]
    pub fn delete_price(&mut self, price_id: PriceId) -> Promise {
        self.promise_catalog_admin_on_price(price_id, |expected_caller, price_id| {
            ext_self_prices::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .delete_price_after_get_owner(price_id, expected_caller)
        })
    }

    // Private callbacks after pool `get_owner_id`.

    #[private]
    pub fn create_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        name: String,
        description: String,
        amount: U128,
        price_type: PriceType,
        billing_period: Option<BillingPeriod>,
        lock_factor_near_months: U128,
        metadata: Option<PriceMetadata>,
        expected_caller: AccountId,
    ) -> PriceId {
        let mut product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            product.status == CatalogStatus::Active,
            "This product is archived or inactive"
        );
        self.validate_price_fields(
            price_type,
            billing_period,
            lock_factor_near_months,
            amount,
            metadata.as_ref(),
        );
        if price_type == PriceType::Farm {
            self.require_no_active_farm_price_for_product(&product);
        }

        let price_id = next_unique_price_id(self);
        let price = Price {
            price_id: price_id.clone(),
            product_id: product_id.clone(),
            name,
            description,
            amount,
            price_type,
            billing_period,
            lock_factor_near_months,
            metadata: metadata.clone(),
            status: CatalogStatus::Active,
            usage_count: 0,
        };
        self.internal_set_price(price_id.clone(), price);
        if price_type == PriceType::Farm {
            let reward_rate = metadata
                .and_then(|m| m.farm_reward_rate)
                .expect("Farm price metadata must contain farm_reward_rate");
            let pool = FarmPool {
                price_id: price_id.clone(),
                product_id: product_id.clone(),
                reward_rate,
                total_farm_shares: U128(0),
                acc_reward_per_share: U128(0),
                last_reward_settle_ns: U64(crate::utils::block_timestamp()),
            };
            self.internal_set_farm_pool(price_id.clone(), pool);
        }
        product.price_ids.push(price_id.clone());
        self.internal_set_product(product_id, product);
        price_id
    }

    #[private]
    pub fn edit_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        name: Option<String>,
        description: Option<String>,
        metadata: Option<PriceMetadata>,
        expected_caller: AccountId,
    ) {
        let mut price = self.require_price(&price_id);
        let product = self.require_product(&price.product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);

        if let Some(name) = name {
            price.name = name;
        }
        if let Some(description) = description {
            price.description = description;
        }

        if let Some(metadata) = metadata {
            match price.price_type {
                PriceType::Farm => {
                    if let Some(max_amount) = metadata.max_amount {
                        require!(
                            max_amount.0 >= price.amount.0,
                            "Price max_amount must be greater than or equal to amount"
                        );
                    }
                    let old_rate = price.metadata.as_ref().and_then(|m| m.farm_reward_rate);
                    let new_rate = metadata
                        .farm_reward_rate
                        .unwrap_or_else(|| old_rate.expect("Farm price must have a reward rate"));
                    if Some(new_rate) != old_rate {
                        self.settle_farm_pool(&price_id);
                        let mut pool = self.require_farm_pool(&price_id);
                        pool.reward_rate = new_rate;
                        self.internal_set_farm_pool(price_id.clone(), pool);
                        crate::events::log_farm_reward_rate_update(&price_id, new_rate.0);
                    }
                    price.metadata = Some(PriceMetadata {
                        max_amount: metadata.max_amount,
                        farm_reward_rate: Some(new_rate),
                    });
                }
                PriceType::OneOff | PriceType::Recurring => {
                    require!(
                        metadata.farm_reward_rate.is_none(),
                        "farm_reward_rate is only valid for Farm prices"
                    );
                    if let Some(max_amount) = metadata.max_amount {
                        require!(
                            max_amount.0 >= price.amount.0,
                            "Price max_amount must be greater than or equal to amount"
                        );
                    }
                    price.metadata = Some(metadata);
                }
            }
        }

        self.internal_set_price(price_id, price);
    }

    #[private]
    pub fn archive_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    ) {
        let mut price = self.require_price(&price_id);
        let product = self.require_product(&price.product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        self.assert_no_pending_update_references_price(&price_id);
        if price.price_type == PriceType::Farm {
            let pool = self.require_farm_pool(&price_id);
            require!(
                pool.total_farm_shares.0 == 0,
                "Cannot archive a farm price while it has active stake"
            );
        }
        let product_id = price.product_id.clone();
        price.status = CatalogStatus::Archived;
        self.internal_set_price(price_id.clone(), price);
        // Default must reference an active tier; see `set_product_default_price`.
        self.clear_product_default_price_field_if_matches(&product_id, &price_id);
    }

    #[private]
    pub fn delete_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    ) {
        let price = self.require_price(&price_id);
        let mut product = self.require_product(&price.product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            price.usage_count == 0,
            "Cannot delete this price while it is in use"
        );
        if price.price_type == PriceType::Farm {
            if let Some(pool) = self.internal_get_farm_pool(&price_id) {
                require!(
                    pool.total_farm_shares.0 == 0,
                    "Cannot delete a farm price while it has active stake"
                );
                self.farm_pools.remove(&price_id);
            }
        }
        self.assert_no_pending_update_references_price(&price_id);
        let product_id = price.product_id.clone();
        product.price_ids.retain(|x| x != &price_id);
        self.internal_set_product(price.product_id.clone(), product);
        self.prices.remove(&price_id);
        self.clear_product_default_price_field_if_matches(&product_id, &price_id);
    }

    #[private]
    pub fn unarchive_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        price_id: PriceId,
        expected_caller: AccountId,
    ) {
        let mut price = self.require_price(&price_id);
        let product = self.require_product(&price.product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            price.status == CatalogStatus::Archived,
            "Price is not archived"
        );
        if price.price_type == PriceType::Farm {
            self.require_no_active_farm_price_for_product(&product);
        }
        price.status = CatalogStatus::Active;
        self.internal_set_price(price_id, price);
    }

    // Public price view functions.

    pub fn get_price(&self, price_id: PriceId) -> Option<Price> {
        self.internal_get_price(&price_id)
    }
}

impl Contract {
    pub(crate) fn internal_get_price(&self, id: &PriceId) -> Option<Price> {
        self.prices.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_price(&mut self, id: PriceId, price: Price) {
        self.prices.insert(id, price.into());
    }

    pub(crate) fn require_price(&self, price_id: &PriceId) -> Price {
        self.internal_get_price(price_id)
            .unwrap_or_else(|| env::panic_str("Price not found in the catalog"))
    }

    fn validate_price_fields(
        &self,
        price_type: PriceType,
        billing_period: Option<BillingPeriod>,
        lock_factor_near_months: U128,
        amount: U128,
        metadata: Option<&PriceMetadata>,
    ) {
        if let Some(max_amount) = metadata.and_then(|m| m.max_amount) {
            require!(
                max_amount.0 >= amount.0,
                "Price max_amount must be greater than or equal to amount"
            );
        }
        match price_type {
            PriceType::OneOff => {
                require!(
                    billing_period.is_none(),
                    "One-off price must not set billing_period"
                );
                require!(
                    metadata.and_then(|m| m.farm_reward_rate).is_none(),
                    "farm_reward_rate is only valid for Farm prices"
                );
            }
            PriceType::Recurring => {
                require!(
                    billing_period == Some(BillingPeriod::Monthly),
                    "Recurring price must use monthly billing_period"
                );
                require!(
                    metadata.and_then(|m| m.farm_reward_rate).is_none(),
                    "farm_reward_rate is only valid for Farm prices"
                );
            }
            PriceType::Farm => {
                require!(
                    billing_period.is_none(),
                    "Farm price must not set billing_period"
                );
                require!(
                    lock_factor_near_months.0 == 0,
                    "Farm price lock_factor_near_months must be zero"
                );
                // TODO: split farm minimum stake from `min_lock_amount` once launch values are finalized.
                require!(
                    amount.0 >= self.internal_get_config().min_lock_amount.as_yoctonear(),
                    "Farm price amount is below min_lock_amount"
                );
                require!(
                    metadata.and_then(|m| m.farm_reward_rate).is_some(),
                    "Farm price requires farm_reward_rate"
                );
            }
        }
    }

    pub(crate) fn require_price_and_product(&self, price_id: &PriceId) -> (Price, Product) {
        let price = self.require_price(price_id);
        let product = self.require_product(&price.product_id);
        (price, product)
    }

    pub(crate) fn get_active_price_and_product(&self, price_id: &PriceId) -> (Price, Product) {
        let price = self.require_price(price_id);
        require!(
            price.status == CatalogStatus::Active,
            "This price is not active; pick an active price"
        );
        let product = self.require_product(&price.product_id);
        require!(
            product.status == CatalogStatus::Active,
            "This product is not active; pick an active product"
        );
        (price, product)
    }

    pub(crate) fn require_recurring_monthly_price(&self, price: &Price) {
        require!(
            price.price_type == PriceType::Recurring,
            "This price is not a recurring subscription price"
        );
        require!(
            price.billing_period == Some(BillingPeriod::Monthly),
            "Only monthly billing is supported"
        );
    }

    /// Resolve price → product → pool, run catalog admin preamble, then `get_owner_id` → `build_tail(caller, price_id)`.
    pub(crate) fn promise_catalog_admin_on_price(
        &self,
        price_id: PriceId,
        build_tail: impl FnOnce(AccountId, PriceId) -> Promise,
    ) -> Promise {
        let (_, product) = self.require_price_and_product(&price_id);
        let (validator_id, expected_caller) =
            self.catalog_admin_entry_for_pool(&product.validator_id);
        Self::promise_pool_get_owner_id_then(validator_id, build_tail(expected_caller, price_id))
    }
}
