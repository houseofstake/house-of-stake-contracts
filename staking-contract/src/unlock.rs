use crate::utils::block_timestamp;
use crate::*;
use near_sdk::{Promise, assert_one_yocto, env, near, require};

#[near]
impl Contract {
    /// User-driven unlock: only the lock owner, after `lock.end_ns`. Runs the same per-epoch validator
    /// pipeline as catalog **`lock`** (balance sync / withdraw / settle when due, or fast path when this
    /// pool already settled the current NEAR epoch), then **`internal_unstake`** (queues `pending_to_unstake`;
    /// pool `unstake` follows on the next settlement-driven flow or **`epoch_settle`**).
    #[payable]
    pub fn unlock(&mut self, lock_id: LockId) -> Promise {
        self.require_enough_gas_for_epoch_settlement();
        assert_one_yocto();
        self.assert_not_paused();

        let buyer = env::predecessor_account_id();
        let lock = self.require_lock_owned_by(
            &lock_id,
            &buyer,
            "Lock not found; check the lock id",
            "Only the lock owner can unlock",
        );
        require!(lock.status == LockStatus::Active, "Lock is not active");
        self.assert_subscription_lock_can_unlock(&lock);
        require!(
            block_timestamp() >= lock.end_ns.0,
            "Lock period has not ended yet"
        );

        let validator_id = lock.validator_id.clone();
        let _validator = self.require_validator_idle(&validator_id);

        self.promise_validator_per_epoch_settlement_then(
            validator_id.clone(),
            UserAction::UnlockQueueUnstake {
                validator_id,
                lock_id,
                account_id: lock.account_id.clone(),
                shares_remove: lock.shares.0,
            },
        )
    }
}

// Epoch pipeline: unlock / unstake tail callbacks.

#[near]
impl Contract {
    /// **[Pipeline 5b]** Share exit after pre-user settlement (**0–3**); pool `unstake` for this exit is deferred.
    #[private]
    pub fn resolve_unlock(
        &mut self,
        lock_id: LockId,
        account_id: AccountId,
        validator_id: ValidatorId,
        shares_remove: u128,
    ) {
        let _validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );

        let mut lock = self.require_lock(&lock_id, "Lock not found");
        require!(
            lock.account_id == account_id,
            "Unlock no longer matches the lock owner; retry"
        );
        require!(
            lock.status == LockStatus::Active,
            "Lock is no longer active; nothing to unlock"
        );
        require!(lock.validator_id == validator_id, "Lock validator mismatch");
        require!(lock.shares.0 == shares_remove, "Lock shares changed; retry");

        self.internal_unstake(account_id.clone(), validator_id.clone(), shares_remove);
        lock.status = LockStatus::UnlockRequested;
        crate::events::log_unlock(
            lock_id.as_str(),
            &lock.account_id.clone(),
            &lock.validator_id.clone(),
        );
        self.internal_set_lock(lock_id.clone(), lock);
    }
}

impl Contract {
    pub(crate) fn require_lock(&self, lock_id: &LockId, not_found_msg: &str) -> Lock {
        self.internal_get_lock(lock_id)
            .unwrap_or_else(|| env::panic_str(not_found_msg))
    }

    pub(crate) fn require_lock_owned_by(
        &self,
        lock_id: &LockId,
        owner: &AccountId,
        not_found_msg: &str,
        owner_mismatch_msg: &str,
    ) -> Lock {
        let lock = self.require_lock(lock_id, not_found_msg);
        require!(lock.account_id == *owner, owner_mismatch_msg);
        lock
    }

    pub(crate) fn require_subscription_lock_owned_by(
        &self,
        sub: &Subscription,
        buyer: &AccountId,
    ) -> Lock {
        let lock = self
            .internal_get_lock(&sub.last_lock_id)
            .unwrap_or_else(|| env::panic_str("No lock is linked to this subscription"));
        require!(
            lock.account_id == *buyer,
            "Only the lock owner can change this subscription lock"
        );
        require!(lock.status == LockStatus::Active, "Lock is not active");
        lock
    }

    /// Subscription-only unlock guard. Active auto-renewing subscriptions must be cancelled
    /// (`cancel_at_period_end`) before stake can be unlocked; a past `lock.end_ns` alone is not enough.
    /// No-op for one-off locks, missing subscription records, or subscriptions already winding down.
    fn assert_subscription_lock_can_unlock(&self, lock: &Lock) {
        let OrderRef::Subscription {
            subscription_id, ..
        } = &lock.order
        else {
            return;
        };
        let Some(sub) = self.internal_get_subscription(subscription_id) else {
            return;
        };
        if sub.status != SubscriptionStatus::Active || sub.cancel_at_period_end {
            return;
        }
        let projected = self.project_subscription_view_now(sub);
        require!(
            self.subscription_now(subscription_id) >= projected.end_ns.0,
            "Active subscription lock cannot be unlocked; cancel the subscription first"
        );
    }
}
