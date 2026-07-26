use crate::utils::{NS_PER_DAY_TIMESTAMP, block_timestamp, check_near_price_lock};
use crate::*;
use near_sdk::json_types::{U64, U128};
use near_sdk::{AccountId, NearToken, PromiseOrValue, env, near, require};

/// Stripe-style **billing anchor day** (1–31). Not the real UTC calendar day-of-month; it is a stable
/// fingerprint from block time until civil-calendar billing is implemented.
fn anchor_day_from_timestamp(ts: u64) -> u8 {
    let d = (ts / NS_PER_DAY_TIMESTAMP) % 31;
    (d as u8 + 1).min(31)
}

#[near]
impl Contract {
    /// Lock NEAR for a catalog price. Attach the NEAR to lock.
    ///
    /// Provide **exactly one** of **`price_id`** or **`product_id`**:
    /// - **`price_id: Some`**, **`product_id: null`** — lock using that catalog price (same as always).
    /// - **`price_id: null`**, **`product_id: Some`** — lock using [`Product::default_price_id`](crate::types::Product::default_price_id) for that product (`set_product_default_price`).
    ///
    /// One-off prices require `duration_ns`. Recurring monthly subscription prices must omit
    /// `duration_ns`; the lock duration is derived from the subscription billing period.
    #[payable]
    pub fn lock(
        &mut self,
        price_id: Option<PriceId>,
        product_id: Option<ProductId>,
        duration_ns: Option<U64>,
    ) -> PromiseOrValue<LockId> {
        let resolved = self.resolve_price_id_for_lock(price_id, product_id);
        self.lock_with_price_id(resolved, duration_ns)
    }

    fn lock_with_price_id(
        &mut self,
        price_id: PriceId,
        duration_ns: Option<U64>,
    ) -> PromiseOrValue<LockId> {
        self.require_enough_gas_for_epoch_settlement();
        let (buyer, locked) = self.lock_entry_preamble();
        let (price, product) = self.get_active_price_and_product(&price_id);

        match price.price_type {
            PriceType::OneOff => {
                let duration_ns = duration_ns.unwrap_or_else(|| {
                    env::panic_str("duration_ns is required for one-off prices")
                });
                self.lock_one_off_with_catalog(buyer, locked, price, product, duration_ns)
            }
            PriceType::Recurring => {
                require!(
                    duration_ns.is_none(),
                    "duration_ns must be omitted for recurring subscription prices"
                );
                self.lock_recurring_subscription_with_catalog(buyer, locked, price, product)
            }
            PriceType::Farm => env::panic_str("Use stake for farm prices"),
        }
    }

    fn lock_one_off_with_catalog(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        price: Price,
        product: Product,
        duration_ns: U64,
    ) -> PromiseOrValue<LockId> {
        let dur = duration_ns.0;
        require!(
            dur >= self.internal_get_config().min_lock_duration_ns.0
                && dur <= self.internal_get_config().max_lock_duration_ns.0,
            "Lock duration is outside the allowed range for this contract"
        );
        require!(
            price.price_type == PriceType::OneOff,
            "This price is not a one-off product price"
        );
        require!(
            price.billing_period.is_none(),
            "One-off price must not set billing_period"
        );

        let validator_id = product.validator_id.clone();
        self.assert_validator_active_for_lock(&validator_id);

        let dur_u128 = u128::from(dur);
        check_near_price_lock(&price, locked.as_yoctonear(), dur_u128)
            .unwrap_or_else(|e| env::panic_str(e));

        let order = OrderRef::ProductPurchase {
            product_id: product.product_id.clone(),
            price_id: price.price_id.clone(),
        };
        let _validator = self.require_validator_idle(&validator_id);
        // WASM production: [`Contract::promise_validator_per_epoch_settlement_then`] then mint (`epoch.rs`).
        // Host targets (`tests/*.rs`, `cargo check` on the host triple): `near_sdk::testing_env!` does not run
        // returned promise chains—use synchronous commit (`finalize_lock` → `commit_catalog_lock`).
        // The library is built **without** `cfg(test)` for integration tests, so this split uses `target_arch`
        // (not `cfg(test)`): WASM builds always use the real promise path.
        #[cfg(not(target_arch = "wasm32"))]
        {
            return PromiseOrValue::Value(
                self.finalize_lock(buyer, price, product, locked, dur_u128, order),
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            return self
                .promise_validator_per_epoch_settlement_then(
                    validator_id.clone(),
                    UserAction::CommitLock {
                        validator_id,
                        buyer,
                        locked,
                        duration_ns: dur_u128,
                        order,
                    },
                )
                .into();
        }
    }

    fn lock_recurring_subscription_with_catalog(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        price: Price,
        product: Product,
    ) -> PromiseOrValue<LockId> {
        self.require_recurring_monthly_price(&price);

        let validator_id = product.validator_id.clone();
        self.assert_validator_active_for_lock(&validator_id);
        let price_id = price.price_id.clone();

        self.validate_subscription_target_amount(&price, U128(locked.as_yoctonear()));

        // Same host synchronous path as the one-off branch (see comment there).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _validator = self.require_validator_idle(&validator_id);
            return PromiseOrValue::Value(self.commit_recurring_subscription_lock_after_settle(
                buyer,
                locked,
                price_id,
                validator_id,
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            return self
                .promise_validator_per_epoch_settlement_then(
                    validator_id.clone(),
                    UserAction::CommitRecurringSubscriptionLock {
                        validator_id,
                        buyer,
                        locked,
                        price_id,
                    },
                )
                .into();
        }
    }

    fn commit_recurring_subscription_lock_after_settle(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> LockId {
        let (price, product) = self.get_active_price_and_product(&price_id);
        self.require_recurring_monthly_price(&price);
        require!(
            product.validator_id == validator_id,
            "Catalog validator for this price does not match the pool used for this lock"
        );
        self.assert_validator_active_for_lock(&validator_id);

        let product_id = product.product_id.clone();
        let sub_key = (buyer.clone(), product_id.clone());
        let now = block_timestamp();

        let existing_subscription_id = self
            .subscription_by_account_product
            .get(&sub_key)
            .cloned()
            .or_else(|| {
                self.find_subscription_by_projected_product(&buyer, &product_id)
                    .map(|found| found.subscription_id)
            });
        if existing_subscription_id.is_none() {
            self.assert_product_not_reserved_by_pending_update(&buyer, &product_id, None);
        }

        let mut subscription_now = now;
        let (subscription, sub_id, is_new_index) = if let Some(sid) = existing_subscription_id {
            self.apply_due_subscription_update(&sid);
            let mut sub = self.require_subscription_by_id(&sid);
            subscription_now = self.subscription_now(&sid);
            require!(
                sub.account_id == buyer,
                "Only the subscription owner can perform this action"
            );
            if subscription_now < sub.end_ns.0 {
                if let Some(prev) = self.internal_get_lock(&sub.last_lock_id) {
                    require!(
                        prev.status != LockStatus::Active,
                        "This subscription period already has an active lock"
                    );
                }
                (sub, sid, false)
            } else if sub.cancel_at_period_end {
                // Period has ended with cancel-at-end: remove stale index and subscription so this call
                // creates a fresh subscription (same path as first-time subscribe).
                let old_sub_key = (buyer.clone(), sub.product_id.clone());
                self.subscription_by_account_product.remove(&old_sub_key);
                self.remove_subscription_from_indexes(&buyer, &sid, true);
                self.internal_remove_subscription(&sid);
                let (sid_new, sub_new) =
                    self.new_subscription_for_lock(&buyer, &product, &price_id, now);
                subscription_now = now;
                (sub_new, sid_new, true)
            } else {
                // Renewal window: extend billing period for the current effective tier.
                let start = sub.end_ns.0.max(subscription_now);
                let end = crate::subscriptions::add_months_stripe_style(sub.anchor_day, 1, start);
                sub.start_ns = U64(start);
                sub.end_ns = U64(end);
                sub.status = SubscriptionStatus::Active;
                (sub, sid, false)
            }
        } else {
            let (sid, sub) = self.new_subscription_for_lock(&buyer, &product, &price_id, now);
            (sub, sid, true)
        };

        self.validate_subscription_target_amount(&price, U128(locked.as_yoctonear()));

        if !is_new_index {
            require!(
                price_id == subscription.price_id,
                "price_id must match current subscription tier"
            );
            if let Some(prev) = self.internal_get_lock(&subscription.last_lock_id) {
                require!(
                    locked.as_yoctonear() == prev.amount_near.as_yoctonear(),
                    "Locked NEAR must match current subscription stake amount"
                );
            }
        }

        require!(
            subscription.end_ns.0 > subscription_now,
            "Subscription billing period has already ended"
        );
        let duration_ns = u128::from(subscription.end_ns.0.saturating_sub(subscription_now));
        require!(duration_ns > 0, "Lock duration must be positive");

        check_near_price_lock(&price, locked.as_yoctonear(), duration_ns)
            .unwrap_or_else(|e| env::panic_str(e));

        let order = OrderRef::Subscription {
            subscription_id: sub_id.clone(),
            price_id: price_id.clone(),
            period_start_ns: subscription.start_ns,
            period_end_ns: subscription.end_ns,
        };

        self.commit_catalog_lock(
            buyer,
            locked,
            duration_ns,
            order,
            validator_id,
            Some((subscription, sub_id, is_new_index)),
        )
    }

    pub fn get_lock(&self, lock_id: LockId) -> Option<Lock> {
        self.internal_get_lock(&lock_id)
    }
}

impl Contract {
    pub(crate) fn internal_get_lock(&self, id: &LockId) -> Option<Lock> {
        self.locks.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_lock(&mut self, id: LockId, lock: Lock) {
        self.locks.insert(id, lock.into());
    }

    pub(crate) fn lock_entry_preamble(&self) -> (AccountId, NearToken) {
        self.assert_not_paused();
        let buyer = env::predecessor_account_id();
        self.ensure_min_storage_for_new_lock(&buyer);

        let locked = env::attached_deposit();
        require!(
            locked.as_yoctonear() >= self.internal_get_config().min_lock_amount.as_yoctonear(),
            "Attached NEAR is below the contract minimum lock amount (min_lock_amount)"
        );
        (buyer, locked)
    }

    /// Picks the catalog price id for a lock from caller input.
    ///
    /// Exactly one of `price_id` or `product_id` must be `Some`. If only `product_id` is given,
    /// the product's default catalog price is used; that price must belong to the same product.
    pub(crate) fn resolve_price_id_for_lock(
        &self,
        price_id: Option<PriceId>,
        product_id: Option<ProductId>,
    ) -> PriceId {
        match (price_id, product_id) {
            // Use the explicitly chosen price; active-catalog checks happen when the lock is applied.
            (Some(pid), None) => pid,
            (None, Some(prod_id)) => {
                // Resolve via the product's default_price_id, then sanity-check the price.
                let pr_id = self
                    .internal_get_product(&prod_id)
                    .and_then(|p| p.default_price_id.clone())
                    .unwrap_or_else(|| env::panic_str("No default price for this product"));
                let pr = self.require_price(&pr_id);
                require!(
                    pr.product_id == prod_id,
                    "Default price does not belong to this product"
                );
                pr_id
            }
            // Both `price_id` and `product_id` — caller must pick one.
            (Some(_), Some(_)) => env::panic_str("Provide only one of price_id or product_id"),
            // Neither identifier given.
            (None, None) => env::panic_str("Provide price_id or product_id"),
        }
    }

    fn new_subscription_for_lock(
        &mut self,
        buyer: &AccountId,
        product: &Product,
        price_id: &PriceId,
        start_ns: u64,
    ) -> (SubscriptionId, Subscription) {
        let anchor = anchor_day_from_timestamp(start_ns);
        let end_ns = crate::subscriptions::add_months_stripe_style(anchor, 1, start_ns);
        let subscription_id = crate::ids::next_subscription_id(&mut self.id_nonce);
        let subscription = Subscription {
            subscription_id: subscription_id.clone(),
            account_id: buyer.clone(),
            product_id: product.product_id.clone(),
            price_id: price_id.clone(),
            start_ns: U64(start_ns),
            end_ns: U64(end_ns),
            anchor_day: anchor,
            last_lock_id: String::new(),
            status: SubscriptionStatus::Active,
            cancel_at_period_end: false,
            pending_update: None,
        };
        (subscription_id, subscription)
    }

    /// Commits catalog-lock **state**: mints pool shares for `locked` NEAR, stores the lock,
    /// bumps catalog usage, updates validator pending stake and per-buyer share balance, then
    /// optionally updates subscription storage (subscription + `(account, product)` index when new).
    ///
    /// **Inputs:** Re-reads active catalog price/product from `order` and requires
    /// `product.validator_id == validator_id` so the mint matches the pool used on the lock path.
    ///
    /// **When to call:** Stake figures passed into [`mint_shares`] must match pool reality. Production
    /// invokes this from [`crate::epoch::Contract::resolve_lock`] after
    /// the shared per-epoch pre-user settlement pipeline (**0–3**) on the lock promise chain.
    pub(crate) fn commit_catalog_lock(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        duration_ns: u128,
        order: OrderRef,
        validator_id: ValidatorId,
        subscription_followup: Option<(Subscription, SubscriptionId, bool)>,
    ) -> LockId {
        // Catalog line item for this lock (one-off or subscription).
        let price_id = match &order {
            OrderRef::ProductPurchase { price_id, .. }
            | OrderRef::Subscription { price_id, .. } => price_id.clone(),
        };
        let (mut price, mut product) = self.get_active_price_and_product(&price_id);
        require!(
            product.validator_id == validator_id,
            "Catalog validator for this price does not match the pool used for this lock"
        );

        let new_shares = self.internal_stake(&buyer, &validator_id, locked);

        // Persist the lock (duration → `end_ns`); `order` ties billing/catalog to this stake.
        let lock_id = crate::ids::next_lock_id(&mut self.id_nonce);
        let lock = Lock {
            lock_id: lock_id.clone(),
            account_id: buyer.clone(),
            validator_id: validator_id.clone(),
            amount_near: locked,
            shares: U128(new_shares),
            start_ns: U64(block_timestamp()),
            end_ns: U64(
                block_timestamp().saturating_add(u64::try_from(duration_ns).unwrap_or(u64::MAX))
            ),
            order: order.clone(),
            status: LockStatus::Active,
        };
        self.internal_set_lock(lock_id.clone(), lock);

        // Catalog usage counters + persist updated price, product, and validator state.
        price.usage_count = price.usage_count.saturating_add(1);
        product.usage_count = product.usage_count.saturating_add(1);

        self.internal_set_price(price.price_id.clone(), price);
        self.internal_set_product(product.product_id.clone(), product);

        // Drives prepaid lock storage (`per_lock_storage_stake` × count) for this account.
        let user_lock_count_before = self.user_lock_count.get(&buyer).copied().unwrap_or(0);
        self.user_lock_count
            .insert(buyer.clone(), user_lock_count_before.saturating_add(1));

        // Subscription path: caller already built/updated `Subscription`; we only persist + optional index.
        if let Some((mut subscription, sub_id, is_new_index)) = subscription_followup {
            subscription.last_lock_id = lock_id.clone();
            let sub_key = (
                subscription.account_id.clone(),
                subscription.product_id.clone(),
            );
            self.internal_set_subscription(sub_id.clone(), subscription);
            self.add_subscription_to_indexes(&buyer, &sub_id, is_new_index);
            if is_new_index {
                self.subscription_by_account_product
                    .insert(sub_key, sub_id.clone());
            }
        }

        crate::events::log_lock(lock_id.as_str(), &buyer);

        lock_id
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Host-only: skips pool balance refresh and stake promises (see module comment on `lock`).
    pub(crate) fn finalize_lock(
        &mut self,
        buyer: AccountId,
        _price: Price,
        product: Product,
        locked: NearToken,
        duration_ns: u128,
        order: OrderRef,
    ) -> LockId {
        self.commit_catalog_lock(
            buyer,
            locked,
            duration_ns,
            order,
            product.validator_id.clone(),
            None,
        )
    }
}

// Epoch pipeline: catalog mint tail callback.

#[near]
impl Contract {
    /// **[Pipeline 5a]** Catalog mint after **4**. Pre-user settlement (**0–3**) already ran before
    /// mint; this lock's `pending_to_stake` is queued for a later `unlock` / `withdraw` / `epoch_settle`.
    /// Returns `lock_id` so user lock calls can decode the minted lock id on WASM.
    #[private]
    pub fn resolve_lock(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        duration_ns: u128,
        order: OrderRef,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<LockId> {
        let lock_id = self.commit_catalog_lock(
            buyer,
            locked,
            duration_ns,
            order,
            validator_id.clone(),
            None,
        );
        let _validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );
        PromiseOrValue::Value(lock_id)
    }

    /// **[Pipeline 5a]** Recurring subscription lock after **4**. Subscription
    /// renewal/new-period state is resolved only after validator settlement.
    #[private]
    pub fn resolve_recurring_subscription_lock_after_settle(
        &mut self,
        buyer: AccountId,
        locked: NearToken,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<LockId> {
        let _validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );
        PromiseOrValue::Value(self.commit_recurring_subscription_lock_after_settle(
            buyer,
            locked,
            price_id,
            validator_id,
        ))
    }
}
