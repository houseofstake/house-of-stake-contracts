use crate::*;
use near_sdk::{NearToken, Promise, assert_one_yocto, env, near, require};

#[near]
impl Contract {
    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: self.internal_get_config().min_storage_deposit,
            max: None,
        }
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id)
    }

    /// NEP-145: attach NEAR to register or top up prepaid storage.
    ///
    /// Storage is retained as the base registration deposit plus configured per-lock,
    /// per-farm-position, and per-purchase surcharges for records created by the account.
    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.assert_not_paused();
        let attached = env::attached_deposit();
        let predecessor = env::predecessor_account_id();
        let account_id = account_id.unwrap_or_else(|| predecessor.clone());
        let registration_only = registration_only.unwrap_or(false);

        let current = self.internal_get_account(&account_id);
        let current_total = current
            .as_ref()
            .map(|account| account.storage_deposit.as_yoctonear())
            .unwrap_or(0);
        let min = self.storage_balance_bounds().min.as_yoctonear();

        let accepted_yocto = if registration_only {
            min.saturating_sub(current_total)
                .min(attached.as_yoctonear())
        } else {
            require!(attached.as_yoctonear() > 0, "Attach NEAR for storage");
            attached.as_yoctonear()
        };

        let new_total = current_total
            .checked_add(accepted_yocto)
            .expect("Storage deposit overflow; reduce the attached amount");
        require!(
            new_total >= min,
            "Attached deposit is less than the minimum storage balance"
        );

        let mut account = current.unwrap_or_default();
        account.storage_deposit = NearToken::from_yoctonear(new_total);
        self.internal_set_account(account_id.clone(), account);

        let refund_yocto = attached.as_yoctonear().saturating_sub(accepted_yocto);
        if refund_yocto > 0 {
            Promise::new(predecessor).transfer(NearToken::from_yoctonear(refund_yocto));
        }

        self.internal_storage_balance_of(&account_id)
            .expect("Account was just registered")
    }

    /// NEP-145: withdraw available prepaid storage.
    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        assert_one_yocto();
        self.assert_not_paused();

        let account_id = env::predecessor_account_id();
        let mut account = self
            .internal_get_account(&account_id)
            .expect("Account not registered; call storage_deposit first");

        let available_yocto = self.available_storage_yocto(&account_id, &account);
        let withdraw_yocto = match amount {
            Some(amount) => {
                require!(
                    amount.as_yoctonear() > 0,
                    "Withdraw amount must be greater than zero"
                );
                require!(
                    amount.as_yoctonear() <= available_yocto,
                    "Withdraw exceeds available storage"
                );
                amount.as_yoctonear()
            }
            None => available_yocto,
        };

        if withdraw_yocto == 0 {
            return self
                .internal_storage_balance_of(&account_id)
                .expect("Registered account must have a storage balance");
        }

        account.storage_deposit = account
            .storage_deposit
            .checked_sub(NearToken::from_yoctonear(withdraw_yocto))
            .expect("Internal error: storage withdraw amount was not bounded correctly");
        self.internal_set_account(account_id.clone(), account);

        Promise::new(account_id.clone()).transfer(NearToken::from_yoctonear(withdraw_yocto));

        self.internal_storage_balance_of(&account_id)
            .expect("Registered account must have a storage balance")
    }

    /// NEP-145: unregister an account when no per-record storage is retained.
    ///
    /// `force=true` is intentionally rejected because later claim/withdraw paths need account
    /// registration to remain present while locks, purchases, or subscriptions are retained.
    #[payable]
    pub fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        self.assert_not_paused();
        require!(!force.unwrap_or(false), "Force unregister is not supported");

        let account_id = env::predecessor_account_id();
        let Some(account) = self.internal_get_account(&account_id) else {
            return false;
        };

        if self.has_retained_record_storage(&account_id) {
            return false;
        }

        self.accounts.remove(&account_id);
        if account.storage_deposit.as_yoctonear() > 0 {
            Promise::new(account_id).transfer(account.storage_deposit);
        }
        true
    }

    pub fn get_account(&self, account_id: AccountId) -> Option<Account> {
        self.internal_get_account(&account_id)
    }
}

impl Contract {
    pub(crate) fn internal_get_account(&self, id: &AccountId) -> Option<Account> {
        self.accounts.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_account(&mut self, id: AccountId, account: Account) {
        self.accounts.insert(id, account.into());
    }

    fn internal_storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        let account = self.internal_get_account(account_id)?;
        Some(StorageBalance {
            total: account.storage_deposit,
            available: NearToken::from_yoctonear(
                self.available_storage_yocto(account_id, &account),
            ),
        })
    }

    fn available_storage_yocto(&self, account_id: &AccountId, account: &Account) -> u128 {
        account
            .storage_deposit
            .as_yoctonear()
            .saturating_sub(self.required_storage_deposit_yocto(account_id, 0, 0, 0))
    }

    fn has_retained_record_storage(&self, account_id: &AccountId) -> bool {
        self.user_lock_count.get(account_id).copied().unwrap_or(0) > 0
            || self
                .user_purchase_count
                .get(account_id)
                .copied()
                .unwrap_or(0)
                > 0
            || self
                .user_farm_position_count
                .get(account_id)
                .copied()
                .unwrap_or(0)
                > 0
            || self
                .subscriptions_by_account
                .get(account_id)
                .map(|subscription_ids| !subscription_ids.is_empty())
                .unwrap_or(false)
            || self
                .farm_accounts
                .get(account_id)
                .map(|account| {
                    let account: FarmAccount = account.clone().into();
                    account.active_position_count > 0
                })
                .unwrap_or(false)
            || self.has_pending_unstake_tranches(account_id)
    }

    fn has_pending_unstake_tranches(&self, account_id: &AccountId) -> bool {
        self.user_pending_unstake_validator_count
            .get(account_id)
            .copied()
            .unwrap_or(0)
            > 0
    }

    pub(crate) fn set_user_pending_unstake_tranches(
        &mut self,
        account_validator_key: (AccountId, ValidatorId),
        tranches: Vec<PendingUnstakeTranche>,
    ) {
        let account_id = account_validator_key.0.clone();
        let had_tranches = self
            .user_pending_unstake
            .get(&account_validator_key)
            .map(|existing| !existing.is_empty())
            .unwrap_or(false);
        let has_tranches = !tranches.is_empty();

        if has_tranches {
            self.user_pending_unstake
                .insert(account_validator_key, tranches);
        } else {
            self.user_pending_unstake.remove(&account_validator_key);
        }

        match (had_tranches, has_tranches) {
            (false, true) => self.increment_pending_unstake_validator_count(&account_id),
            (true, false) => self.decrement_pending_unstake_validator_count(&account_id),
            _ => {}
        }
    }

    fn increment_pending_unstake_validator_count(&mut self, account_id: &AccountId) {
        let next = self
            .user_pending_unstake_validator_count
            .get(account_id)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .expect("pending unstake validator count overflow");
        self.user_pending_unstake_validator_count
            .insert(account_id.clone(), next);
    }

    fn decrement_pending_unstake_validator_count(&mut self, account_id: &AccountId) {
        match self
            .user_pending_unstake_validator_count
            .get(account_id)
            .copied()
            .unwrap_or(0)
        {
            0 | 1 => {
                self.user_pending_unstake_validator_count.remove(account_id);
            }
            count => {
                self.user_pending_unstake_validator_count
                    .insert(account_id.clone(), count - 1);
            }
        }
    }

    fn require_registered_account(&self, account_id: &AccountId) -> Account {
        self.internal_get_account(account_id)
            .expect("Account not registered; call storage_deposit")
    }

    fn assert_storage_deposit_at_least(
        &self,
        account: &Account,
        required_yocto: u128,
        err_msg: &str,
    ) {
        require!(
            account.storage_deposit.as_yoctonear() >= required_yocto,
            err_msg
        );
    }

    /// `extra_locks` / `extra_purchases` / `extra_farm_positions` = additional records we are about to add.
    pub(crate) fn required_storage_deposit_yocto(
        &self,
        account_id: &AccountId,
        extra_locks: u32,
        extra_purchases: u32,
        extra_farm_positions: u32,
    ) -> u128 {
        let base = self
            .internal_get_config()
            .min_storage_deposit
            .as_yoctonear();
        let per_lock = self
            .internal_get_config()
            .per_lock_storage_stake
            .as_yoctonear();
        let per_farm_position = self
            .internal_get_config()
            .per_farm_position_storage_stake
            .as_yoctonear();
        let per_purchase = self
            .internal_get_config()
            .per_purchase_storage_stake
            .as_yoctonear();
        let recorded_lock_count =
            self.user_lock_count.get(account_id).copied().unwrap_or(0) as u128;
        let recorded_purchase_count = self
            .user_purchase_count
            .get(account_id)
            .copied()
            .unwrap_or(0) as u128;
        let recorded_farm_position_count = self
            .user_farm_position_count
            .get(account_id)
            .copied()
            .unwrap_or(0) as u128;
        let total_locks = recorded_lock_count.saturating_add(u128::from(extra_locks));
        let total_purchases = recorded_purchase_count.saturating_add(u128::from(extra_purchases));
        let total_farm_positions =
            recorded_farm_position_count.saturating_add(u128::from(extra_farm_positions));
        base.saturating_add(per_lock.saturating_mul(total_locks))
            .saturating_add(per_purchase.saturating_mul(total_purchases))
            .saturating_add(per_farm_position.saturating_mul(total_farm_positions))
    }

    /// Account registered and still meets global [`crate::config::Config::min_storage_deposit`] only (no per-lock surcharge).
    /// Use for claim / withdraw paths so older locks do not force endless storage top-ups.
    pub(crate) fn ensure_min_base_storage(&self, account_id: &AccountId) {
        let account = self.require_registered_account(account_id);
        let min = self
            .internal_get_config()
            .min_storage_deposit
            .as_yoctonear();
        self.assert_storage_deposit_at_least(&account, min, "Top up storage (minimum prepaid)");
    }

    /// Before creating a lock: require prepaid storage for one more lock entry.
    pub(crate) fn ensure_min_storage_for_new_lock(&self, account_id: &AccountId) {
        let account = self.require_registered_account(account_id);
        let need = self.required_storage_deposit_yocto(account_id, 1, 0, 0);
        self.assert_storage_deposit_at_least(&account, need, "Top up storage for another lock");
    }

    /// Before creating a direct purchase: require prepaid storage for one more purchase entry.
    pub(crate) fn ensure_min_storage_for_new_purchase(&self, account_id: &AccountId) {
        let account = self.require_registered_account(account_id);
        let need = self.required_storage_deposit_yocto(account_id, 0, 1, 0);
        self.assert_storage_deposit_at_least(&account, need, "Top up storage for another purchase");
    }

    /// Before creating a farm position: require prepaid storage for one more retained position entry.
    pub(crate) fn ensure_min_storage_for_new_farm_position(&self, account_id: &AccountId) {
        let account = self.require_registered_account(account_id);
        let need = self.required_storage_deposit_yocto(account_id, 0, 0, 1);
        self.assert_storage_deposit_at_least(
            &account,
            need,
            "Top up storage for another farm position",
        );
    }
}
