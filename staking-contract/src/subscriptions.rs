//! Subscription billing helpers (Stripe-style linear months) and subscription **lifecycle** RPCs
//! (`cancel_subscription`, `update_subscription`, …). Subscription **locking** (`lock`)
//! stays in [`crate::lock`] because it shares the pool refresh / mint pipeline with product locks.

use crate::utils::{AVG_MONTH_NS, block_timestamp, check_near_price_lock, near_from_shares};
use crate::*;
use common::U256;
use near_sdk::borsh::BorshSerialize;
use near_sdk::json_types::{U64, U128};
use near_sdk::store::{IterableSet, LookupMap};
use near_sdk::{AccountId, NearToken, PromiseOrValue, assert_one_yocto, env, near, require};

#[cfg(feature = "test")]
const TEST_SUBSCRIPTION_TIMESTAMP_PREFIX: &[u8] = b"_test_subscription_timestamp_";

/// Extend `from_ns` by `months` × average Gregorian months (linear approximation).
/// `anchor_day` is validated but not yet applied.
pub fn add_months_stripe_style(anchor_day: u8, months: u32, from_ns: u64) -> u64 {
    let _anchor_day = anchor_day.clamp(1, 31);
    let add_ns = (months as u128).saturating_mul(AVG_MONTH_NS);
    let add_u64 = u64::try_from(add_ns).unwrap_or(u64::MAX);
    from_ns.saturating_add(add_u64)
}

struct SubscriptionUpdateInputs {
    sub: Subscription,
    current_price: Price,
    target_price: Price,
    target_product: Product,
    lock: Lock,
    now_ns: u64,
}

struct SubscriptionUpdateDecision {
    immediate_plan_change: bool,
    immediate_stake_increase: Option<NearToken>,
    pending_plan_change: bool,
    pending_stake_decrease_target: Option<NearToken>,
    pending_apply_ns: Option<U64>,
}

impl SubscriptionUpdateDecision {
    fn has_immediate_change(&self) -> bool {
        self.immediate_plan_change || self.immediate_stake_increase.is_some()
    }

    fn has_pending_change(&self) -> bool {
        self.pending_plan_change || self.pending_stake_decrease_target.is_some()
    }
}

pub(crate) struct ProjectedSubscriptionLookup {
    pub(crate) subscription_id: SubscriptionId,
    projected: Subscription,
}

#[near]
impl Contract {
    /// Stop renewing after the current billing period. The active lock remains until `lock.end_ns`; use
    /// [`crate::unlock::Contract::unlock`] afterwards. Attach 1 yocto.
    #[payable]
    pub fn cancel_subscription(&mut self, subscription_id: SubscriptionId) {
        assert_one_yocto();
        self.assert_not_paused();
        let buyer = env::predecessor_account_id();
        let sub = self.require_subscription_owned_by_id(&buyer, &subscription_id);
        let old_product_id = sub.product_id.clone();
        // Normalize stale active windows before marking cancel-at-end so stored `end_ns`
        // represents the current virtual billing period boundary.
        let mut sub = self.project_subscription_view_now(sub);
        Self::assert_subscription_active(&sub);
        Self::clear_pending_update(&mut sub);
        self.sync_subscription_lock_window(&sub);
        if old_product_id != sub.product_id {
            self.move_subscription_product_index(
                &buyer,
                &subscription_id,
                &old_product_id,
                &sub.product_id,
            );
        }
        let product_id = sub.product_id.clone();
        let price_id = sub.price_id.clone();
        sub.cancel_at_period_end = true;
        self.internal_set_subscription(subscription_id.clone(), sub.clone());
        crate::events::log_subscription_cancel(&buyer, &subscription_id, &product_id, &price_id);
    }

    /// Undo [`Contract::cancel_subscription`] before the current billing period ends: clear `cancel_at_period_end`
    /// so renewals resume normally after `end_ns`. Attach 1 yocto.
    #[payable]
    pub fn resume_subscription(&mut self, subscription_id: SubscriptionId) {
        assert_one_yocto();
        self.assert_not_paused();
        let buyer = env::predecessor_account_id();
        let mut sub = self.require_subscription_owned_by_id(&buyer, &subscription_id);
        let product_id = sub.product_id.clone();
        let price_id = sub.price_id.clone();
        Self::assert_subscription_active(&sub);
        require!(
            sub.cancel_at_period_end,
            "Subscription is not scheduled to cancel at period end"
        );
        require!(
            self.subscription_now(&subscription_id) < sub.end_ns.0,
            "Current billing period has ended; subscribe again with lock instead"
        );
        sub.cancel_at_period_end = false;
        self.internal_set_subscription(subscription_id.clone(), sub.clone());
        crate::events::log_subscription_resume(&buyer, &subscription_id, &product_id, &price_id);
    }

    /// Update a subscription to a target recurring tier and explicit target stake amount.
    ///
    /// Stake increases apply immediately after the shared validator settlement pipeline. Stake decreases
    /// are scheduled for the next billing period, projected in views after the apply timestamp, and
    /// lazily committed on the next related mutation.
    #[payable]
    pub fn update_subscription(
        &mut self,
        subscription_id: SubscriptionId,
        target_price_id: PriceId,
        target_amount: U128,
    ) -> PromiseOrValue<SubscriptionPlanChangeOutcome> {
        self.assert_not_paused();
        let buyer = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        let inputs = self.checked_subscription_update_inputs(
            &buyer,
            &target_price_id,
            target_amount,
            &subscription_id,
            None,
        );

        let validator_id = inputs.target_product.validator_id.clone();
        self.assert_validator_active_for_lock(&validator_id);

        #[cfg(not(target_arch = "wasm32"))]
        {
            return PromiseOrValue::Value(self.commit_subscription_update_after_settle(
                buyer,
                deposit,
                target_price_id,
                target_amount,
                subscription_id,
                validator_id,
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.require_enough_gas_for_epoch_settlement();
            let _validator = self.require_validator_idle(&validator_id);
            return self
                .promise_validator_per_epoch_settlement_then(
                    validator_id.clone(),
                    UserAction::SubscriptionUpdate {
                        validator_id,
                        buyer,
                        deposit,
                        target_price_id,
                        target_amount,
                        subscription_id,
                    },
                )
                .into();
        }
    }

    // -------------------------------------------------------------------------
    // Public subscription view functions
    // -------------------------------------------------------------------------

    pub fn get_subscription(&self, subscription_id: SubscriptionId) -> Option<Subscription> {
        self.internal_get_subscription(&subscription_id)
            .map(|sub| self.project_subscription_view_now(sub))
    }

    /// Lookup subscription by account and catalog product (at most one subscription per product).
    pub fn get_subscription_for_product(
        &self,
        account_id: AccountId,
        product_id: ProductId,
    ) -> Option<Subscription> {
        self.find_subscription_by_projected_product(&account_id, &product_id)
            .map(|found| found.projected)
    }

    pub fn get_subscription_for_price(
        &self,
        account_id: AccountId,
        price_id: PriceId,
    ) -> Option<Subscription> {
        let price = self.internal_get_price(&price_id)?;
        self.get_subscription_for_product(account_id, price.product_id.clone())
    }

    pub fn get_subscriptions_for_account(
        &self,
        account_id: AccountId,
        from_index: u64,
        limit: u64,
    ) -> Vec<Subscription> {
        let ids = self.subscription_ids_for_account_view(&account_id);
        self.collect_paginated(from_index, limit, ids.len() as u64, |index| {
            ids.get(index as usize)
                .and_then(|id| self.internal_get_subscription(id))
                .map(|sub| self.project_subscription_view_now(sub))
        })
    }

    pub fn get_subscriptions(&self, from_index: u64, limit: u64) -> Vec<Subscription> {
        let skip = usize::try_from(from_index).unwrap_or(usize::MAX);
        let take = usize::try_from(limit).unwrap_or(usize::MAX);
        self.subscription_ids
            .keys()
            .skip(skip)
            .take(take)
            .filter_map(|id| self.internal_get_subscription(id))
            .map(|sub| self.project_subscription_view_now(sub))
            .collect()
    }

    pub fn get_subscriptions_for_product(
        &self,
        product_id: ProductId,
        from_index: u64,
        limit: u64,
    ) -> Vec<Subscription> {
        let Some(ids) = self.subscriptions_by_product.get(&product_id) else {
            return Vec::new();
        };
        let skip = usize::try_from(from_index).unwrap_or(usize::MAX);
        let take = usize::try_from(limit).unwrap_or(usize::MAX);
        ids.iter()
            .skip(skip)
            .take(take)
            .filter_map(|id| self.internal_get_subscription(id))
            .map(|sub| self.project_subscription_view_now(sub))
            .collect()
    }
}

// Epoch pipeline: subscription update tail callback.

#[near]
impl Contract {
    /// **[Pipeline 5d]** Subscription update after pre-user settlement (**4**).
    #[private]
    pub fn on_subscription_update_after_settle(
        &mut self,
        buyer: AccountId,
        deposit: NearToken,
        target_price_id: PriceId,
        target_amount: U128,
        subscription_id: SubscriptionId,
        validator_id: ValidatorId,
    ) -> PromiseOrValue<SubscriptionPlanChangeOutcome> {
        let outcome = self.commit_subscription_update_after_settle(
            buyer,
            deposit,
            target_price_id,
            target_amount,
            subscription_id,
            validator_id.clone(),
        );
        let _validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );
        // Pre-user settlement (**0–3**) already ran; new `pending_to_stake` from the update
        // is queued for the next user action or `epoch_settle` (same as **5a** catalog mint).
        PromiseOrValue::Value(outcome)
    }
}

impl Contract {
    pub(crate) fn internal_get_subscription(&self, id: &SubscriptionId) -> Option<Subscription> {
        self.subscriptions.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_subscription(
        &mut self,
        id: SubscriptionId,
        subscription: Subscription,
    ) {
        let old = self.internal_get_subscription(&id);
        if let Some(old) = old.as_ref() {
            self.remove_pending_update_target_refs(old);
        }
        self.add_pending_update_target_refs(&subscription);
        self.subscriptions.insert(id, subscription.into());
    }

    pub(crate) fn internal_remove_subscription(&mut self, id: &SubscriptionId) {
        if let Some(old) = self.internal_get_subscription(id) {
            self.remove_pending_update_target_refs(&old);
        }
        self.subscriptions.remove(id.as_str());
    }

    fn add_pending_update_target_refs(&mut self, sub: &Subscription) {
        let Some(price_id) = Self::pending_update_target_price_id(sub) else {
            return;
        };
        Self::increment_count(&mut self.pending_update_target_price_counts, &price_id);
        if let Some(price) = self.internal_get_price(&price_id) {
            Self::increment_count(
                &mut self.pending_update_target_product_counts,
                &price.product_id,
            );
        }
    }

    fn remove_pending_update_target_refs(&mut self, sub: &Subscription) {
        let Some(price_id) = Self::pending_update_target_price_id(sub) else {
            return;
        };
        Self::decrement_count(&mut self.pending_update_target_price_counts, &price_id);
        if let Some(price) = self.internal_get_price(&price_id) {
            Self::decrement_count(
                &mut self.pending_update_target_product_counts,
                &price.product_id,
            );
        }
    }

    fn pending_update_target_price_id(sub: &Subscription) -> Option<PriceId> {
        sub.pending_update
            .as_ref()
            .and_then(|pending| pending.target_price_id.clone())
    }

    fn increment_count<K>(counts: &mut LookupMap<K, u32>, key: &K)
    where
        K: Clone + Ord + BorshSerialize,
    {
        let next = counts.get(key).copied().unwrap_or(0).saturating_add(1);
        counts.insert(key.clone(), next);
    }

    fn decrement_count<K>(counts: &mut LookupMap<K, u32>, key: &K)
    where
        K: Clone + Ord + BorshSerialize,
    {
        let Some(current) = counts.get(key).copied() else {
            return;
        };
        if current <= 1 {
            counts.remove(key);
        } else {
            counts.insert(key.clone(), current - 1);
        }
    }

    pub(crate) fn add_subscription_to_indexes(
        &mut self,
        account_id: &AccountId,
        subscription_id: &SubscriptionId,
        add_global: bool,
    ) {
        let mut ids = self
            .subscriptions_by_account
            .get(account_id)
            .cloned()
            .unwrap_or_default();
        if ids.iter().any(|id| id == subscription_id) {
            return;
        }

        ids.push(subscription_id.clone());
        self.subscriptions_by_account
            .insert(account_id.clone(), ids);
        self.add_subscription_to_product_index(account_id, subscription_id);

        if add_global {
            self.subscription_ids.insert(subscription_id.clone(), ());
        }
    }

    pub(crate) fn remove_subscription_from_indexes(
        &mut self,
        account_id: &AccountId,
        subscription_id: &SubscriptionId,
        remove_global: bool,
    ) {
        let Some(mut ids) = self.subscriptions_by_account.get(account_id).cloned() else {
            return;
        };
        let before = ids.len();
        ids.retain(|id| id != subscription_id);
        if ids.len() == before {
            return;
        }

        if ids.is_empty() {
            self.subscriptions_by_account.remove(account_id);
        } else {
            self.subscriptions_by_account
                .insert(account_id.clone(), ids);
        }
        self.remove_subscription_from_product_index(subscription_id);

        if !remove_global {
            return;
        }

        self.subscription_ids.remove(subscription_id);
    }

    fn add_subscription_to_product_index(
        &mut self,
        account_id: &AccountId,
        subscription_id: &SubscriptionId,
    ) {
        let Some(subscription) = self.internal_get_subscription(subscription_id) else {
            return;
        };
        // The caller supplies the account index owner; verify it matches the
        // stored subscription before mirroring that subscription into a product index.
        if subscription.account_id != *account_id {
            return;
        }

        self.add_subscription_to_product_index_for_product(
            &subscription.product_id,
            subscription_id,
        );
    }

    fn add_subscription_to_product_index_for_product(
        &mut self,
        product_id: &ProductId,
        subscription_id: &SubscriptionId,
    ) {
        if let Some(ids) = self.subscriptions_by_product.get_mut(product_id) {
            ids.insert(subscription_id.clone());
            return;
        }

        let mut ids = IterableSet::new(Self::subscriptions_by_product_set_key(product_id));
        ids.insert(subscription_id.clone());
        self.subscriptions_by_product
            .insert(product_id.clone(), ids);
    }

    fn remove_subscription_from_product_index(&mut self, subscription_id: &SubscriptionId) {
        let Some(subscription) = self.internal_get_subscription(subscription_id) else {
            return;
        };
        self.remove_subscription_from_product_index_for_product(
            &subscription.product_id,
            subscription_id,
        );
    }

    fn remove_subscription_from_product_index_for_product(
        &mut self,
        product_id: &ProductId,
        subscription_id: &SubscriptionId,
    ) {
        let should_remove_product = {
            let Some(ids) = self.subscriptions_by_product.get_mut(product_id) else {
                return;
            };
            ids.remove(subscription_id);
            ids.is_empty()
        };
        if should_remove_product {
            self.subscriptions_by_product.remove(product_id);
        }
    }

    pub(crate) fn subscriptions_by_product_set_key(product_id: &ProductId) -> StorageKeys {
        StorageKeys::SubscriptionsByProductSet {
            product_hash: env::sha256(product_id.as_bytes()),
        }
    }

    pub(crate) fn assert_no_pending_update_references_price(&self, price_id: &PriceId) {
        require!(
            self.pending_update_target_price_counts
                .get(price_id)
                .copied()
                .unwrap_or(0)
                == 0,
            "Cannot archive or delete this price while it is referenced by a pending subscription update"
        );
    }

    pub(crate) fn assert_no_pending_update_references_product(&self, product_id: &ProductId) {
        require!(
            self.pending_update_target_product_counts
                .get(product_id)
                .copied()
                .unwrap_or(0)
                == 0,
            "Cannot archive or delete this product while it is referenced by a pending subscription update"
        );
    }

    fn subscription_ids_for_account_view(&self, account_id: &AccountId) -> Vec<SubscriptionId> {
        self.subscriptions_by_account
            .get(account_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn find_subscription_by_projected_product(
        &self,
        buyer: &AccountId,
        product_id: &ProductId,
    ) -> Option<ProjectedSubscriptionLookup> {
        if let Some(sid) = self
            .subscription_by_account_product
            .get(&(buyer.clone(), product_id.clone()))
            .cloned()
        {
            let stored = self.require_subscription_by_id(&sid);
            require!(
                stored.account_id == *buyer,
                "Only the subscription owner can perform this action"
            );
            let projected = self.project_subscription_view_now(stored.clone());
            if projected.product_id == *product_id {
                return Some(ProjectedSubscriptionLookup {
                    subscription_id: sid,
                    projected,
                });
            }
        }

        self.subscription_ids_for_account_view(buyer)
            .into_iter()
            .find_map(|sid| {
                let stored = self.require_subscription_by_id(&sid);
                if stored.account_id != *buyer || stored.status != SubscriptionStatus::Active {
                    return None;
                }
                let projected = self.project_subscription_view_now(stored.clone());
                (projected.product_id == *product_id).then_some(ProjectedSubscriptionLookup {
                    subscription_id: sid,
                    projected,
                })
            })
    }

    pub(crate) fn move_subscription_product_index(
        &mut self,
        buyer: &AccountId,
        subscription_id: &SubscriptionId,
        old_product_id: &ProductId,
        new_product_id: &ProductId,
    ) {
        if old_product_id == new_product_id {
            return;
        }

        let new_key = (buyer.clone(), new_product_id.clone());
        if let Some(existing) = self.subscription_by_account_product.get(&new_key) {
            require!(
                existing == subscription_id,
                "Subscription already exists for target product"
            );
        } else {
            self.subscription_by_account_product
                .insert(new_key, subscription_id.clone());
        }

        let old_key = (buyer.clone(), old_product_id.clone());
        if let Some(existing) = self.subscription_by_account_product.get(&old_key) {
            if existing == subscription_id {
                self.subscription_by_account_product.remove(&old_key);
            }
        }
        self.remove_subscription_from_product_index_for_product(old_product_id, subscription_id);
        self.add_subscription_to_product_index_for_product(new_product_id, subscription_id);
    }

    pub(crate) fn require_subscription_by_id(
        &self,
        subscription_id: &SubscriptionId,
    ) -> Subscription {
        self.internal_get_subscription(subscription_id)
            .unwrap_or_else(|| env::panic_str("Subscription not found"))
    }

    pub(crate) fn require_subscription_owned_by_id(
        &self,
        buyer: &AccountId,
        subscription_id: &SubscriptionId,
    ) -> Subscription {
        let sub = self.require_subscription_by_id(subscription_id);
        require!(
            sub.account_id == *buyer,
            "Only the subscription owner can perform this action"
        );
        sub
    }

    pub(crate) fn assert_subscription_active(sub: &Subscription) {
        require!(
            sub.status == SubscriptionStatus::Active,
            "This subscription is not active (cancelled, expired, or not yet started)"
        );
    }

    pub(crate) fn validate_subscription_target_amount(&self, price: &Price, target_amount: U128) {
        require!(
            target_amount.0 >= price.amount.0,
            "Target stake amount is below the price minimum"
        );
        if let Some(max_amount) = price.metadata.as_ref().and_then(|m| m.max_amount) {
            require!(
                target_amount.0 <= max_amount.0,
                "Target stake amount is above the price maximum"
            );
        }
    }

    pub(crate) fn assert_product_not_reserved_by_pending_update(
        &self,
        buyer: &AccountId,
        product_id: &ProductId,
        allowed_subscription_id: Option<&SubscriptionId>,
    ) {
        let reserved = self
            .subscription_ids_for_account_view(buyer)
            .into_iter()
            .filter(|sid| {
                allowed_subscription_id
                    .map(|allowed| sid != allowed)
                    .unwrap_or(true)
            })
            .filter_map(|sid| self.internal_get_subscription(&sid))
            .filter(|sub| sub.status == SubscriptionStatus::Active)
            .filter_map(|sub| {
                sub.pending_update
                    .and_then(|pending| pending.target_price_id)
            })
            .filter_map(|target_price_id| self.internal_get_price(&target_price_id))
            .any(|price| price.product_id == *product_id);

        require!(
            !reserved,
            "Subscription already has a pending update for target product"
        );
    }

    fn clear_pending_update(sub: &mut Subscription) {
        sub.pending_update = None;
    }

    fn projected_subscription_window_from(
        &self,
        anchor_day: u8,
        mut start: u64,
        now: u64,
    ) -> (U64, U64) {
        let mut end = add_months_stripe_style(anchor_day, 1, start);
        while now >= end {
            start = end;
            end = add_months_stripe_style(anchor_day, 1, start);
        }
        (U64(start), U64(end))
    }

    pub(crate) fn apply_due_subscription_update(
        &mut self,
        subscription_id: &SubscriptionId,
    ) -> bool {
        let Some(stored) = self.internal_get_subscription(subscription_id) else {
            return false;
        };
        if stored.status != SubscriptionStatus::Active || stored.cancel_at_period_end {
            return false;
        }
        let Some(pending) = stored.pending_update.clone() else {
            return false;
        };
        let apply_ns = pending.apply_ns;
        let now = self.subscription_now(subscription_id);
        if now < apply_ns.0 {
            return false;
        }

        let target_price = pending
            .target_price_id
            .as_ref()
            .map(|target_price_id| self.require_price(target_price_id));
        let target_product = target_price
            .as_ref()
            .map(|price| self.require_product(&price.product_id));
        let old_product_id = stored.product_id.clone();
        let new_product_id = target_product
            .as_ref()
            .map(|product| product.product_id.clone())
            .unwrap_or_else(|| stored.product_id.clone());
        let buyer = stored.account_id.clone();

        if let Some(target_amount) = pending.target_amount {
            #[cfg(target_arch = "wasm32")]
            {
                if let Some(lock) = self.internal_get_lock(&stored.last_lock_id) {
                    let validator = self.require_validator(&lock.validator_id);
                    require!(
                        validator.last_settlement_epoch >= env::epoch_height(),
                        "Pending stake decrease requires validator settlement before apply"
                    );
                }
            }
            self.apply_scheduled_stake_decrease_at_renewal(&buyer, &stored, target_amount);
        }

        let (period_start_ns, period_end_ns) =
            self.projected_subscription_window_from(stored.anchor_day, apply_ns.0, now);

        let mut updated = stored;
        if let Some(target_price) = target_price.as_ref() {
            updated.product_id = new_product_id.clone();
            updated.price_id = target_price.price_id.clone();
        }
        updated.start_ns = period_start_ns;
        updated.end_ns = period_end_ns;
        Self::clear_pending_update(&mut updated);
        self.sync_subscription_lock_window(&updated);
        if old_product_id != new_product_id {
            self.move_subscription_product_index(
                &buyer,
                subscription_id,
                &old_product_id,
                &new_product_id,
            );
        }
        let final_product_id = updated.product_id.clone();
        let final_price_id = updated.price_id.clone();
        let last_lock_id = updated.last_lock_id.clone();
        self.internal_set_subscription(subscription_id.clone(), updated);
        if let (Some(mut target_price), Some(mut target_product)) = (target_price, target_product) {
            target_price.usage_count = target_price.usage_count.saturating_add(1);
            target_product.usage_count = target_product.usage_count.saturating_add(1);
            self.internal_set_price(target_price.price_id.clone(), target_price);
            self.internal_set_product(target_product.product_id.clone(), target_product);
        }
        let lock_amount = self
            .internal_get_lock(&last_lock_id)
            .map(|lock| lock.amount_near.as_yoctonear())
            .unwrap_or_default();
        crate::events::log_subscription_update(
            &buyer,
            subscription_id,
            &final_product_id,
            &final_price_id,
            lock_amount,
            "applied_pending_update",
        );
        true
    }

    fn sync_subscription_lock_window(&mut self, sub: &Subscription) {
        let Some(mut lock) = self.internal_get_lock(&sub.last_lock_id) else {
            return;
        };
        if lock.account_id != sub.account_id || lock.status != LockStatus::Active {
            return;
        }

        Self::sync_lock_window_fields(&mut lock, sub);
        self.internal_set_lock(lock.lock_id.clone(), lock);
    }

    fn sync_lock_window_fields(lock: &mut Lock, sub: &Subscription) {
        lock.start_ns = sub.start_ns;
        lock.end_ns = sub.end_ns;
        if let OrderRef::Subscription {
            subscription_id,
            price_id,
            period_start_ns,
            period_end_ns,
        } = &mut lock.order
        {
            if *subscription_id == sub.subscription_id {
                *price_id = sub.price_id.clone();
                *period_start_ns = sub.start_ns;
                *period_end_ns = sub.end_ns;
            }
        }
    }

    fn checked_subscription_update_inputs(
        &self,
        buyer: &AccountId,
        target_price_id: &PriceId,
        target_amount: U128,
        subscription_id: &SubscriptionId,
        expected_validator_id: Option<&ValidatorId>,
    ) -> SubscriptionUpdateInputs {
        let (target_price, target_product) = self.get_active_price_and_product(target_price_id);
        self.require_recurring_monthly_price(&target_price);
        self.validate_subscription_target_amount(&target_price, target_amount);

        let sub = self.require_subscription_owned_by_id(buyer, subscription_id);
        Self::assert_subscription_active(&sub);
        require!(
            !sub.cancel_at_period_end,
            "Resume subscription before updating"
        );
        if target_product.product_id != sub.product_id {
            let target_key = (buyer.clone(), target_product.product_id.clone());
            if let Some(existing) = self.subscription_by_account_product.get(&target_key) {
                require!(
                    existing == subscription_id,
                    "Subscription already exists for target product"
                );
            }
            self.assert_product_not_reserved_by_pending_update(
                buyer,
                &target_product.product_id,
                Some(subscription_id),
            );
        }
        // Same virtual billing window as [`Contract::get_subscription`] / `get_subscription_for_product`.
        let sub = self.project_subscription_view_now(sub);

        let current_price = self.require_price(&sub.price_id);

        let lock = self.require_subscription_lock_owned_by(&sub, buyer);
        require!(
            target_product.validator_id == lock.validator_id,
            "Target product validator must match subscription lock validator"
        );
        if let Some(expected) = expected_validator_id {
            require!(
                target_product.validator_id == *expected,
                "Catalog validator for this price does not match the pool used for this subscription update"
            );
            require!(
                lock.validator_id == *expected,
                "Subscription lock validator does not match the update validator"
            );
        }

        let now = self.subscription_now(subscription_id);
        require!(
            now < sub.end_ns.0,
            "Current period already ended; renew instead"
        );
        SubscriptionUpdateInputs {
            sub,
            current_price,
            target_price,
            target_product,
            lock,
            now_ns: now,
        }
    }

    fn build_subscription_update_decision(
        &self,
        inputs: &SubscriptionUpdateInputs,
        target_price_id: &PriceId,
        target_amount: U128,
    ) -> SubscriptionUpdateDecision {
        let current_amount = inputs.lock.amount_near.as_yoctonear();
        let target_amount_near = NearToken::from_yoctonear(target_amount.0);
        let plan_changes = *target_price_id != inputs.sub.price_id;
        let plan_direction = inputs
            .target_price
            .amount
            .0
            .cmp(&inputs.current_price.amount.0);
        let stake_direction = target_amount.0.cmp(&current_amount);
        let rem_ns = u128::from(inputs.sub.end_ns.0.saturating_sub(inputs.now_ns));

        // Plan and stake amount can move independently: e.g. upgrade the plan now
        // while scheduling a lower stake amount for the next billing period.
        let immediate_stake_increase = (stake_direction == std::cmp::Ordering::Greater)
            .then(|| NearToken::from_yoctonear(target_amount.0.saturating_sub(current_amount)));
        let pending_stake_decrease_target =
            (stake_direction == std::cmp::Ordering::Less).then_some(target_amount_near);

        let immediate_plan_change = if !plan_changes {
            false
        } else {
            match plan_direction {
                std::cmp::Ordering::Greater => {
                    if pending_stake_decrease_target.is_some() {
                        self.price_supports_amount_for_duration(
                            &inputs.target_price,
                            current_amount,
                            rem_ns,
                        )
                    } else {
                        true
                    }
                }
                std::cmp::Ordering::Equal => {
                    if pending_stake_decrease_target.is_some() {
                        self.price_supports_amount_for_duration(
                            &inputs.target_price,
                            current_amount,
                            rem_ns,
                        )
                    } else {
                        true
                    }
                }
                std::cmp::Ordering::Less => false,
            }
        };
        let pending_plan_change = plan_changes && !immediate_plan_change;

        if immediate_plan_change || immediate_stake_increase.is_some() {
            let immediate_price = if immediate_plan_change {
                &inputs.target_price
            } else {
                &inputs.current_price
            };
            let immediate_amount = if immediate_stake_increase.is_some() {
                target_amount.0
            } else {
                current_amount
            };
            check_near_price_lock(immediate_price, immediate_amount, rem_ns)
                .unwrap_or_else(|e| env::panic_str(e));
        }

        if pending_plan_change || pending_stake_decrease_target.is_some() {
            check_near_price_lock(&inputs.target_price, target_amount.0, AVG_MONTH_NS)
                .unwrap_or_else(|e| env::panic_str(e));
        }

        SubscriptionUpdateDecision {
            immediate_plan_change,
            immediate_stake_increase,
            pending_plan_change,
            pending_stake_decrease_target,
            pending_apply_ns: (pending_plan_change || pending_stake_decrease_target.is_some())
                .then_some(inputs.sub.end_ns),
        }
    }

    fn price_supports_amount_for_duration(
        &self,
        price: &Price,
        amount: u128,
        duration_ns: u128,
    ) -> bool {
        if amount < price.amount.0 {
            return false;
        }
        if let Some(max_amount) = price.metadata.as_ref().and_then(|m| m.max_amount) {
            if amount > max_amount.0 {
                return false;
            }
        }
        check_near_price_lock(price, amount, duration_ns).is_ok()
    }

    fn subscription_update_outcome(
        kind: &str,
        subscription_id: SubscriptionId,
        target_price_id: PriceId,
        target_amount: U128,
        lock_id: Option<LockId>,
        decision: &SubscriptionUpdateDecision,
        current_amount: u128,
    ) -> SubscriptionPlanChangeOutcome {
        SubscriptionPlanChangeOutcome {
            kind: kind.to_string(),
            subscription_id,
            target_price_id,
            target_amount,
            lock_id,
            immediate_plan_change: decision.immediate_plan_change,
            immediate_stake_increase: decision
                .immediate_stake_increase
                .map(|amount| U128(amount.as_yoctonear())),
            pending_plan_change: decision.pending_plan_change,
            pending_stake_decrease: decision
                .pending_stake_decrease_target
                .map(|target| U128(current_amount.saturating_sub(target.as_yoctonear()))),
            pending_apply_ns: decision.pending_apply_ns,
        }
    }

    fn subscription_update_kind(decision: &SubscriptionUpdateDecision) -> &'static str {
        match (
            decision.has_immediate_change(),
            decision.has_pending_change(),
        ) {
            (false, false) => "no_op",
            (true, false) => "changed_immediately",
            (false, true) => "scheduled_for_period_end",
            (true, true) => "changed_immediately_and_scheduled_for_period_end",
        }
    }

    fn apply_subscription_update_state(
        target_price_id: &PriceId,
        decision: &SubscriptionUpdateDecision,
        sub: &mut Subscription,
        target_price: &mut Price,
        target_product: &mut Product,
        lock: &mut Lock,
    ) -> (ProductId, ProductId) {
        let old_product_id = sub.product_id.clone();
        let new_product_id = target_price.product_id.clone();
        Self::clear_pending_update(sub);

        if decision.immediate_plan_change {
            lock.order = OrderRef::Subscription {
                subscription_id: sub.subscription_id.clone(),
                price_id: target_price_id.clone(),
                period_start_ns: sub.start_ns,
                period_end_ns: sub.end_ns,
            };
            sub.product_id = new_product_id.clone();
            sub.price_id = target_price_id.clone();
            target_price.usage_count = target_price.usage_count.saturating_add(1);
            target_product.usage_count = target_product.usage_count.saturating_add(1);
        }

        if decision.has_pending_change() {
            // Deferred changes replace any earlier pending update and become effective
            // at the current billing boundary.
            sub.pending_update = Some(PendingSubscriptionUpdate {
                target_price_id: decision
                    .pending_plan_change
                    .then_some(target_price_id.clone()),
                target_amount: decision.pending_stake_decrease_target,
                apply_ns: decision.pending_apply_ns.expect("pending apply timestamp"),
            });
        }

        Self::sync_lock_window_fields(lock, sub);
        (old_product_id, new_product_id)
    }

    fn commit_subscription_update_after_settle(
        &mut self,
        buyer: AccountId,
        deposit: NearToken,
        target_price_id: PriceId,
        target_amount: U128,
        subscription_id: SubscriptionId,
        expected_validator_id: ValidatorId,
    ) -> SubscriptionPlanChangeOutcome {
        self.apply_due_subscription_update(&subscription_id);
        let inputs = self.checked_subscription_update_inputs(
            &buyer,
            &target_price_id,
            target_amount,
            &subscription_id,
            Some(&expected_validator_id),
        );
        let decision =
            self.build_subscription_update_decision(&inputs, &target_price_id, target_amount);
        let SubscriptionUpdateInputs {
            mut sub,
            mut target_price,
            mut target_product,
            mut lock,
            ..
        } = inputs;
        let current_amount = lock.amount_near.as_yoctonear();

        // The callback is the single mutation point after validator settlement.
        // It revalidates current state, applies stake increases if needed, then
        // commits plan/pending-update changes and storage indexes together.
        let lock_id_out = if let Some(expected_delta) = decision.immediate_stake_increase {
            require!(
                deposit.as_yoctonear() == expected_delta.as_yoctonear(),
                "Attached NEAR must equal the target stake increase"
            );
            require!(
                deposit.as_yoctonear() >= self.internal_get_config().min_lock_amount.as_yoctonear(),
                "Attached NEAR is below the contract minimum lock amount (min_lock_amount)"
            );

            let add_shares = self.internal_stake(&buyer, &expected_validator_id, deposit);
            require!(
                add_shares > 0,
                "Stake increase must mint at least one share"
            );
            lock.amount_near = lock
                .amount_near
                .checked_add(deposit)
                .expect("lock amount_near overflow");
            lock.shares = U128(lock.shares.0.saturating_add(add_shares));
            Some(lock.lock_id.clone())
        } else {
            require!(
                deposit.as_yoctonear() == 1,
                "Requires attached deposit of exactly 1 yoctoNEAR"
            );
            None
        };

        let (old_product_id, new_product_id) = Self::apply_subscription_update_state(
            &target_price_id,
            &decision,
            &mut sub,
            &mut target_price,
            &mut target_product,
            &mut lock,
        );
        self.internal_set_lock(lock.lock_id.clone(), lock);
        if decision.immediate_plan_change {
            self.move_subscription_product_index(
                &buyer,
                &subscription_id,
                &old_product_id,
                &new_product_id,
            );
        }
        self.internal_set_subscription(subscription_id.clone(), sub);
        let target_product_id = target_product.product_id.clone();
        if decision.immediate_plan_change {
            self.internal_set_price(target_price.price_id.clone(), target_price);
            self.internal_set_product(target_product.product_id.clone(), target_product);
        }

        let kind = Self::subscription_update_kind(&decision);
        crate::events::log_subscription_update(
            &buyer,
            &subscription_id,
            &target_product_id,
            &target_price_id,
            target_amount.0,
            kind,
        );
        if let Some(lock_id) = lock_id_out.as_ref() {
            crate::events::log_lock(lock_id.as_str(), &buyer);
        }

        Self::subscription_update_outcome(
            kind,
            subscription_id,
            target_price_id,
            target_amount,
            lock_id_out,
            &decision,
            current_amount,
        )
    }

    /// At scheduled decrease renewal, release surplus stake from the completed subscription lock
    /// as shares → same unstake queue as [`crate::unlock::Contract::unlock`].
    pub(crate) fn apply_scheduled_stake_decrease_at_renewal(
        &mut self,
        buyer: &AccountId,
        sub: &Subscription,
        target_amount: NearToken,
    ) {
        let mut lock = match self.internal_get_lock(&sub.last_lock_id) {
            Some(l) => l,
            None => return,
        };
        if &lock.account_id != buyer || lock.status != LockStatus::Active {
            return;
        }
        let surplus_target = lock
            .amount_near
            .as_yoctonear()
            .saturating_sub(target_amount.as_yoctonear());
        if surplus_target == 0 {
            return;
        }

        let validator_id = lock.validator_id.clone();
        let validator = self.require_validator(&validator_id);
        let net_stake = validator.net_stake_yocto();
        let validator_total_shares = validator.total_shares.0;
        let lock_near_val = near_from_shares(lock.shares.0, net_stake, validator_total_shares);
        if lock_near_val == 0 {
            return;
        }

        let surplus_near = surplus_target.min(lock_near_val);
        let shares_remove = (U256::from(lock.shares.0) * U256::from(surplus_near)
            / U256::from(lock_near_val))
        .as_u128();
        let shares_remove = shares_remove.min(lock.shares.0);
        if shares_remove == 0 {
            return;
        }

        let near_amt = self.internal_unstake(buyer.clone(), validator_id, shares_remove);
        lock.shares = U128(lock.shares.0.saturating_sub(shares_remove));
        let new_amt = lock.amount_near.as_yoctonear().saturating_sub(near_amt);
        lock.amount_near = NearToken::from_yoctonear(new_amt);
        if lock.shares.0 == 0 {
            lock.status = LockStatus::UnlockRequested;
        }
        let lock_id = lock.lock_id.clone();
        self.internal_set_lock(lock.lock_id.clone(), lock);

        crate::events::log_subscription_downgrade_prorate(
            buyer,
            &sub.subscription_id,
            &sub.product_id,
            &sub.price_id,
            &lock_id,
            near_amt,
        );
    }

    pub(crate) fn project_subscription_view_now(&self, mut sub: Subscription) -> Subscription {
        if sub.status != SubscriptionStatus::Active {
            return sub;
        }

        let now = self.subscription_now(&sub.subscription_id);
        if sub.cancel_at_period_end {
            if now >= sub.end_ns.0 {
                sub.status = SubscriptionStatus::Expired;
            }
            return sub;
        }

        let mut projection_start_ns = sub.start_ns.0;
        if let Some(pending) = sub.pending_update.clone() {
            if now >= pending.apply_ns.0 {
                projection_start_ns = pending.apply_ns.0;
                self.project_due_update_fields(&mut sub, &pending);
            }
        }

        let (start_ns, end_ns) =
            self.projected_subscription_window_from(sub.anchor_day, projection_start_ns, now);
        sub.start_ns = start_ns;
        sub.end_ns = end_ns;
        sub
    }

    #[cfg(not(feature = "test"))]
    pub(crate) fn subscription_now(&self, _subscription_id: &SubscriptionId) -> u64 {
        block_timestamp()
    }

    #[cfg(feature = "test")]
    pub(crate) fn subscription_now(&self, subscription_id: &SubscriptionId) -> u64 {
        match env::storage_read(&Self::test_subscription_timestamp_key(subscription_id)) {
            Some(raw) => raw
                .as_slice()
                .try_into()
                .map(u64::from_be_bytes)
                .unwrap_or_else(|_| block_timestamp()),
            _ => block_timestamp(),
        }
    }

    #[cfg(feature = "test")]
    fn test_subscription_timestamp_key(subscription_id: &SubscriptionId) -> Vec<u8> {
        let mut key = TEST_SUBSCRIPTION_TIMESTAMP_PREFIX.to_vec();
        key.extend_from_slice(subscription_id.as_bytes());
        key
    }

    fn project_due_update_fields(
        &self,
        sub: &mut Subscription,
        pending: &PendingSubscriptionUpdate,
    ) {
        if let Some(target_price_id) = pending.target_price_id.clone() {
            let target_price = self.require_price(&target_price_id);
            sub.product_id = target_price.product_id;
            sub.price_id = target_price_id;
        }
        Self::clear_pending_update(sub);
    }
}

#[cfg(feature = "test")]
#[near]
impl Contract {
    /// Test-only helper: make one subscription behave as if `target_timestamp_ns` were the current
    /// block timestamp without changing the global mocked clock or other subscriptions.
    pub fn test_fast_forward_subscription_to(
        &mut self,
        subscription_id: SubscriptionId,
        target_timestamp_ns: U64,
    ) {
        self.require_test_subscription_clock_owner(&subscription_id);
        let now = self.subscription_now(&subscription_id);
        require!(
            target_timestamp_ns.0 >= now,
            "Test clock can only fast-forward subscriptions"
        );
        self.set_test_subscription_timestamp(&subscription_id, target_timestamp_ns.0);
    }

    /// Test-only helper: fast-forward one subscription by `delta_ns` relative to the current
    /// test clock without changing the global mocked clock or other subscriptions.
    pub fn test_fast_forward_subscription_by(
        &mut self,
        subscription_id: SubscriptionId,
        delta_ns: U64,
    ) {
        self.require_test_subscription_clock_owner(&subscription_id);
        let now = self.subscription_now(&subscription_id);
        self.set_test_subscription_timestamp(&subscription_id, now.saturating_add(delta_ns.0));
    }

    /// Test-only helper: account/product convenience wrapper for
    /// [`Contract::test_fast_forward_subscription_to`].
    pub fn test_fast_forward_subscription_for_product_to(
        &mut self,
        product_id: ProductId,
        target_timestamp_ns: U64,
    ) {
        let account_id = env::predecessor_account_id();
        let subscription_id = self
            .find_subscription_by_projected_product(&account_id, &product_id)
            .unwrap_or_else(|| env::panic_str("No subscription for this product; subscribe first"))
            .subscription_id;
        self.test_fast_forward_subscription_to(subscription_id, target_timestamp_ns);
    }

    /// Test-only raw storage view. Public subscription views apply virtual time projection.
    pub fn test_get_stored_subscription(
        &self,
        subscription_id: SubscriptionId,
    ) -> Option<Subscription> {
        self.internal_get_subscription(&subscription_id)
    }

    /// Test-only view: read the timestamp used to project one subscription's current billing window.
    pub fn test_get_subscription_timestamp(&self, subscription_id: SubscriptionId) -> U64 {
        let _ = self.require_subscription_by_id(&subscription_id);
        U64(self.subscription_now(&subscription_id))
    }
}

#[cfg(feature = "test")]
impl Contract {
    fn require_test_subscription_clock_owner(&self, subscription_id: &SubscriptionId) {
        let sub = self.require_subscription_by_id(subscription_id);
        require!(
            sub.account_id == env::predecessor_account_id(),
            "Only the subscription owner can modify its test clock"
        );
    }

    fn set_test_subscription_timestamp(
        &mut self,
        subscription_id: &SubscriptionId,
        timestamp: u64,
    ) {
        let bytes = timestamp.to_be_bytes();
        env::storage_write(
            &Self::test_subscription_timestamp_key(subscription_id),
            &bytes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_month_stack() {
        let out = add_months_stripe_style(15, 2, 100);
        assert_eq!(out, 100 + (2u128 * AVG_MONTH_NS) as u64);
    }
}
