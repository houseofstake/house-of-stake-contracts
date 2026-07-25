//! Catalog products: CRUD, pagination, and default-price binding.
//! Mutating RPCs are gated via `get_owner_id` plus validator owner-or-catalog-manager authorization.
//! Prices live in [`crate::prices`]; this module owns product records and product->price links.

use crate::gas::callbacks;
use crate::utils::block_timestamp;
use crate::*;
use near_sdk::ext_contract;
use near_sdk::json_types::U64;
use near_sdk::{AccountId, Promise, env, near, require};

/// Retry id generation when a collision exists in [`Contract::products`].
fn next_unique_product_id(contract: &mut Contract) -> ProductId {
    crate::ids::next_unique_generated_id(
        &mut contract.id_nonce,
        crate::ids::next_product_id,
        |id| contract.products.contains_key(id),
        "Could not allocate a unique product id; try again",
    )
}

/// Self callbacks for **product** catalog after `get_owner_id` on the staking pool.
#[ext_contract(ext_self_products)]
pub trait ExtSelfProducts {
    fn create_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        validator_id: ValidatorId,
        name: String,
        description: String,
        expected_caller: AccountId,
    ) -> ProductId;
    fn edit_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        name: String,
        description: String,
        expected_caller: AccountId,
    );
    fn archive_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    );
    fn delete_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    );
    fn unarchive_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    );
    fn set_product_default_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        price_id: Option<PriceId>,
        expected_caller: AccountId,
    );
}

#[near]
impl Contract {
    // -------------------------------------------------------------------------
    // Public catalog admin (pool-owner auth via promise chain)
    // -------------------------------------------------------------------------

    /// Register a sellable product on an allowlisted validator pool. Validator owner or catalog manager; attach 1 yocto.
    #[payable]
    pub fn create_product(
        &mut self,
        validator_id: ValidatorId,
        name: String,
        description: String,
    ) -> Promise {
        self.promise_catalog_admin_on_pool(&validator_id, |expected_caller, validator_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .create_product_after_get_owner(validator_id, name, description, expected_caller)
        })
    }

    /// Update product display metadata; does not move the product to another pool.
    #[payable]
    pub fn edit_product(
        &mut self,
        product_id: ProductId,
        name: String,
        description: String,
    ) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .edit_product_after_get_owner(product_id, name, description, expected_caller)
        })
    }

    /// Archive: blocks new locks; clears default price; existing locks/subscriptions unchanged.
    #[payable]
    pub fn archive_product(&mut self, product_id: ProductId) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .archive_product_after_get_owner(product_id, expected_caller)
        })
    }

    /// Remove from storage when never locked and all prices for this product are gone.
    #[payable]
    pub fn delete_product(&mut self, product_id: ProductId) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .delete_product_after_get_owner(product_id, expected_caller)
        })
    }

    /// Restore an archived product for new locks and default-price binding.
    #[payable]
    pub fn unarchive_product(&mut self, product_id: ProductId) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .unarchive_product_after_get_owner(product_id, expected_caller)
        })
    }

    /// Bind or clear the tier used when users call `lock_for_*` with `product_id` only (no `price_id`).
    #[payable]
    pub fn set_product_default_price(
        &mut self,
        product_id: ProductId,
        price_id: Option<PriceId>,
    ) -> Promise {
        self.promise_catalog_admin_on_product(product_id, |expected_caller, product_id| {
            ext_self_products::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .set_product_default_price_after_get_owner(product_id, price_id, expected_caller)
        })
    }

    // -------------------------------------------------------------------------
    // Private callbacks after pool `get_owner_id`
    // -------------------------------------------------------------------------

    #[private]
    pub fn create_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        validator_id: ValidatorId,
        name: String,
        description: String,
        expected_caller: AccountId,
    ) -> ProductId {
        self.assert_validator_catalog_admin(pool_owner, &validator_id, &expected_caller);

        let id = next_unique_product_id(self);
        let product = Product {
            product_id: id.clone(),
            validator_id: validator_id.clone(),
            name,
            description,
            status: CatalogStatus::Active,
            created_ns: U64(block_timestamp()),
            price_ids: Vec::new(),
            default_price_id: None,
            usage_count: 0,
        };
        self.internal_set_product(id.clone(), product);
        self.product_ids.push(id.clone());
        crate::events::log_product_created(id.as_str(), &validator_id);
        id
    }

    #[private]
    pub fn edit_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        name: String,
        description: String,
        expected_caller: AccountId,
    ) {
        let mut product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        product.name = name;
        product.description = description;
        self.internal_set_product(product_id, product);
    }

    #[private]
    pub fn archive_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    ) {
        let mut product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        self.assert_no_pending_update_references_product(&product_id);
        // Archived products cannot serve as default; clear so lock-by-product fails fast.
        product.default_price_id = None;
        product.status = CatalogStatus::Archived;
        self.internal_set_product(product_id, product);
    }

    #[private]
    pub fn delete_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    ) {
        let product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            product.usage_count == 0,
            "Cannot delete this product while it is in use"
        );
        self.assert_no_pending_update_references_product(&product_id);
        require!(
            product.price_ids.is_empty(),
            "Remove or delete all prices for this product first"
        );
        self.remove_product_id_from_list(&product_id);
        self.products.remove(&product_id);
    }

    #[private]
    pub fn unarchive_product_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        expected_caller: AccountId,
    ) {
        let mut product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            product.status == CatalogStatus::Archived,
            "Product is not archived"
        );
        product.status = CatalogStatus::Active;
        self.internal_set_product(product_id, product);
    }

    #[private]
    pub fn set_product_default_price_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        product_id: ProductId,
        price_id: Option<PriceId>,
        expected_caller: AccountId,
    ) {
        let mut product = self.require_product(&product_id);
        self.assert_validator_catalog_admin(pool_owner, &product.validator_id, &expected_caller);
        require!(
            product.status == CatalogStatus::Active,
            "This product is archived or inactive"
        );
        match price_id {
            None => {
                product.default_price_id = None;
            }
            Some(pid) => {
                let catalog_price = self.require_price(&pid);
                require!(
                    catalog_price.product_id == product_id,
                    "Price does not belong to this product"
                );
                require!(
                    catalog_price.status == CatalogStatus::Active,
                    "Only an active (unarchived) price can be the default"
                );
                require!(
                    product.price_ids.iter().any(|x| x == &pid),
                    "Price not listed on product"
                );
                product.default_price_id = Some(pid);
            }
        }
        self.internal_set_product(product_id, product);
    }

    // -------------------------------------------------------------------------
    // Public product view functions
    // -------------------------------------------------------------------------

    pub fn get_product(&self, product_id: ProductId) -> Option<Product> {
        self.internal_get_product(&product_id)
    }

    pub fn get_product_default_price(&self, product_id: ProductId) -> Option<PriceId> {
        self.internal_get_product(&product_id)
            .and_then(|product| product.default_price_id.clone())
    }

    /// Paginated products (stable creation order in [`Contract::product_ids`]).
    pub fn get_products(&self, from_index: u64, limit: u64) -> Vec<Product> {
        let total_len = self.product_ids.len() as u64;
        self.collect_paginated(from_index, limit, total_len, |index| {
            self.product_ids
                .get(index)
                .and_then(|id| self.internal_get_product(id))
        })
    }
}

// Internal helpers (also used from [`crate::prices`]).

impl Contract {
    pub(crate) fn internal_get_product(&self, id: &ProductId) -> Option<Product> {
        self.products.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_product(&mut self, id: ProductId, product: Product) {
        self.products.insert(id, product.into());
    }

    pub(crate) fn require_product(&self, product_id: &ProductId) -> Product {
        self.internal_get_product(product_id)
            .unwrap_or_else(|| env::panic_str("Product not found in the catalog"))
    }

    /// Clears [`Product::default_price_id`] when it references **`price_id`** (e.g. price archived/deleted).
    pub(crate) fn clear_product_default_price_field_if_matches(
        &mut self,
        product_id: &ProductId,
        price_id: &PriceId,
    ) {
        let mut product = match self.internal_get_product(product_id) {
            Some(existing) => existing,
            None => return,
        };
        if product.default_price_id.as_ref() == Some(price_id) {
            product.default_price_id = None;
            self.internal_set_product(product_id.clone(), product);
        }
    }

    /// Resolve product → pool, run catalog admin preamble, then `get_owner_id` → `build_tail(caller, product_id)`.
    pub(crate) fn promise_catalog_admin_on_product(
        &self,
        product_id: ProductId,
        build_tail: impl FnOnce(AccountId, ProductId) -> Promise,
    ) -> Promise {
        let product = self.require_product(&product_id);
        let (validator_id, expected_caller) =
            self.catalog_admin_entry_for_pool(&product.validator_id);
        Self::promise_pool_get_owner_id_then(validator_id, build_tail(expected_caller, product_id))
    }

    /// Catalog admin on a known allowlisted pool (e.g. `create_product` before the product exists in storage).
    pub(crate) fn promise_catalog_admin_on_pool(
        &self,
        validator_id: &ValidatorId,
        build_tail: impl FnOnce(AccountId, ValidatorId) -> Promise,
    ) -> Promise {
        let (validator_id, expected_caller) = self.catalog_admin_entry_for_pool(validator_id);
        Self::promise_pool_get_owner_id_then(
            validator_id.clone(),
            build_tail(expected_caller, validator_id),
        )
    }

    /// Keeps [`Contract::product_ids`] in sync when a product is removed from storage.
    fn remove_product_id_from_list(&mut self, product_id: &ProductId) {
        let len = self.product_ids.len();
        for i in 0..len {
            if self.product_ids.get(i).is_some_and(|id| id == product_id) {
                self.product_ids.drain(i..i + 1);
                return;
            }
        }
    }
}
