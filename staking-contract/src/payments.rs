//! Direct NEAR payments for one-off catalog prices.

use crate::gas::callbacks;
use crate::utils::block_timestamp;
use crate::*;
use near_sdk::ext_contract;
use near_sdk::json_types::U64;
use near_sdk::store::Vector;
use near_sdk::{AccountId, NearToken, Promise, assert_one_yocto, env, near, require};

fn next_unique_purchase_id(contract: &mut Contract) -> PurchaseId {
    crate::ids::next_unique_generated_id(
        &mut contract.id_nonce,
        crate::ids::next_purchase_id,
        |id| contract.purchases.contains_key(id),
        "Could not allocate a unique purchase id; try again",
    )
}

#[ext_contract(ext_self_payments)]
pub trait ExtSelfPayments {
    fn withdraw_revenue_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        validator_id: ValidatorId,
        expected_caller: AccountId,
    ) -> Promise;
}

#[near]
impl Contract {
    /// Directly pay NEAR for a one-off catalog price. This does not create a stake lock.
    #[payable]
    pub fn pay(
        &mut self,
        price_id: Option<PriceId>,
        product_id: Option<ProductId>,
        quantity: U64,
    ) -> PurchaseId {
        self.assert_not_paused();
        require!(quantity.0 > 0, "Quantity must be greater than zero");

        let buyer = env::predecessor_account_id();
        self.ensure_min_storage_for_new_purchase(&buyer);

        let resolved_price_id = self.resolve_price_id_for_lock(price_id, product_id);
        let (mut price, mut product) = self.get_active_price_and_product(&resolved_price_id);
        self.assert_validator_active_for_lock(&product.validator_id);
        require!(
            price.price_type == PriceType::OneOff,
            "This price is not a one-off product price"
        );
        require!(
            price.billing_period.is_none(),
            "One-off price must not set billing_period"
        );

        let expected_amount = price
            .amount
            .0
            .checked_mul(u128::from(quantity.0))
            .expect("Payment amount overflow; reduce the quantity");
        let paid = env::attached_deposit();
        require!(
            paid.as_yoctonear() == expected_amount,
            "Attached deposit must equal price amount times quantity"
        );

        let purchase_id = next_unique_purchase_id(self);
        let purchase = Purchase {
            purchase_id: purchase_id.clone(),
            account_id: buyer.clone(),
            product_id: product.product_id.clone(),
            price_id: price.price_id.clone(),
            quantity,
            amount_paid: paid,
            created_ns: U64(block_timestamp()),
        };
        self.internal_set_purchase(purchase_id.clone(), purchase);
        self.purchase_ids.push(purchase_id.clone());
        self.add_purchase_to_account_index(&buyer, &purchase_id);
        self.add_purchase_to_product_index(&product.product_id, &purchase_id);

        let user_purchase_count_before = self.user_purchase_count.get(&buyer).copied().unwrap_or(0);
        self.user_purchase_count
            .insert(buyer.clone(), user_purchase_count_before.saturating_add(1));

        price.usage_count = price.usage_count.saturating_add(1);
        product.usage_count = product.usage_count.saturating_add(1);

        self.add_revenue_to_validator(&product.validator_id, paid);

        self.internal_set_price(price.price_id.clone(), price.clone());
        self.internal_set_product(product.product_id.clone(), product.clone());

        crate::events::log_payment_create(
            &purchase_id,
            &buyer,
            &product.product_id,
            &price.price_id,
            quantity.0,
            paid.as_yoctonear(),
        );
        purchase_id
    }

    /// Withdraw all direct-payment revenue accrued for a validator. Pool owner only; attach 1 yocto.
    #[payable]
    pub fn withdraw_revenue(&mut self, validator_id: ValidatorId) -> Promise {
        assert_one_yocto();
        self.assert_not_paused();
        let (validator_id, expected_caller) = self.catalog_admin_entry_for_pool(&validator_id);
        Self::promise_pool_get_owner_id_then(
            validator_id.clone(),
            ext_self_payments::ext(env::current_account_id())
                .with_static_gas(callbacks::ON_VALIDATOR_OWNER_CHECK)
                .withdraw_revenue_after_get_owner(validator_id, expected_caller),
        )
    }

    #[private]
    pub fn withdraw_revenue_after_get_owner(
        &mut self,
        #[callback] pool_owner: AccountId,
        validator_id: ValidatorId,
        expected_caller: AccountId,
    ) -> Promise {
        self.assert_validator_owner(pool_owner.clone(), &expected_caller);
        let balance = self
            .revenue_by_validator
            .get(&validator_id)
            .copied()
            .unwrap_or_else(|| NearToken::from_yoctonear(0));
        require!(
            balance.as_yoctonear() > 0,
            "No revenue available to withdraw"
        );

        self.revenue_by_validator
            .insert(validator_id.clone(), NearToken::from_yoctonear(0));

        crate::events::log_revenue_withdraw(&validator_id, &pool_owner, balance.as_yoctonear());
        Promise::new(pool_owner).transfer(balance)
    }

    pub fn get_purchase(&self, purchase_id: PurchaseId) -> Option<Purchase> {
        self.internal_get_purchase(&purchase_id)
    }

    pub fn get_purchases(&self, from_index: u64, limit: u64) -> Vec<Purchase> {
        let total_len = self.purchase_ids.len() as u64;
        self.collect_paginated(from_index, limit, total_len, |index| {
            self.purchase_ids
                .get(index)
                .and_then(|id| self.internal_get_purchase(id))
        })
    }

    pub fn get_purchases_for_account(
        &self,
        account_id: AccountId,
        from_index: u64,
        limit: u64,
    ) -> Vec<Purchase> {
        let Some(ids) = self.purchases_by_account.get(&account_id) else {
            return Vec::new();
        };
        let total_len = u64::from(ids.len());
        self.collect_paginated(from_index, limit, total_len, |index| {
            ids.get(index).and_then(|id| self.internal_get_purchase(id))
        })
    }

    pub fn get_purchases_for_product(
        &self,
        product_id: ProductId,
        from_index: u64,
        limit: u64,
    ) -> Vec<Purchase> {
        let Some(ids) = self.purchases_by_product.get(&product_id) else {
            return Vec::new();
        };
        let total_len = u64::from(ids.len());
        self.collect_paginated(from_index, limit, total_len, |index| {
            ids.get(index).and_then(|id| self.internal_get_purchase(id))
        })
    }

    pub fn get_revenue_balance_for_validator(&self, validator_id: ValidatorId) -> NearToken {
        self.revenue_by_validator
            .get(&validator_id)
            .copied()
            .unwrap_or_else(|| NearToken::from_yoctonear(0))
    }
}

impl Contract {
    pub(crate) fn internal_get_purchase(&self, id: &PurchaseId) -> Option<Purchase> {
        self.purchases.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_purchase(&mut self, id: PurchaseId, purchase: Purchase) {
        self.purchases.insert(id, purchase.into());
    }

    fn add_purchase_to_account_index(&mut self, account_id: &AccountId, purchase_id: &PurchaseId) {
        if let Some(ids) = self.purchases_by_account.get_mut(account_id) {
            ids.push(purchase_id.clone());
            return;
        }

        let mut ids = Vector::new(Self::purchases_by_account_vector_key(account_id));
        ids.push(purchase_id.clone());
        self.purchases_by_account.insert(account_id.clone(), ids);
    }

    fn add_purchase_to_product_index(&mut self, product_id: &ProductId, purchase_id: &PurchaseId) {
        if let Some(ids) = self.purchases_by_product.get_mut(product_id) {
            ids.push(purchase_id.clone());
            return;
        }

        let mut ids = Vector::new(Self::purchases_by_product_vector_key(product_id));
        ids.push(purchase_id.clone());
        self.purchases_by_product.insert(product_id.clone(), ids);
    }

    pub(crate) fn purchases_by_account_vector_key(account_id: &AccountId) -> StorageKeys {
        StorageKeys::PurchasesByAccountVector {
            account_hash: env::sha256(account_id.as_bytes()),
        }
    }

    pub(crate) fn purchases_by_product_vector_key(product_id: &ProductId) -> StorageKeys {
        StorageKeys::PurchasesByProductVector {
            product_hash: env::sha256(product_id.as_bytes()),
        }
    }

    fn add_revenue_to_validator(&mut self, validator_id: &ValidatorId, amount: NearToken) {
        let current = self
            .revenue_by_validator
            .get(validator_id)
            .copied()
            .unwrap_or_else(|| NearToken::from_yoctonear(0));
        let next = current
            .checked_add(amount)
            .expect("Validator revenue overflow; reduce payment amount");
        self.revenue_by_validator.insert(validator_id.clone(), next);
    }
}
