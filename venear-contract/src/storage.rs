use crate::*;
use near_sdk::Promise;

#[near(serializers=[json])]
pub struct StorageBalance {
    pub total: NearToken,
    pub available: NearToken,
}

#[near(serializers=[json])]
pub struct StorageBalanceBounds {
    pub min: NearToken,
    pub max: Option<NearToken>,
}

impl Contract {
    fn internal_storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        if self.accounts.contains_key(account_id) {
            Some(StorageBalance {
                total: self.storage_balance_bounds().min,
                available: NearToken::from_near(0),
            })
        } else {
            None
        }
    }
}

#[near]
impl Contract {
    /// Registers a new account. If the account is already registered, it refunds the attached
    /// deposit.
    /// Requires a deposit of at least `storage_balance_bounds().min`.
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance {
        let amount = env::attached_deposit();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        if self.internal_get_account_internal(&account_id).is_some() {
            env::log_str("The account is already registered, refunding the deposit");
            if amount > NearToken::from_near(0) {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            let min_balance = self.storage_balance_bounds().min;
            if amount < min_balance {
                env::panic_str("The attached deposit is less than the minimum storage balance");
            }

            self.internal_register_account(&account_id, min_balance);
            let refund = amount.saturating_sub(min_balance);
            if refund > NearToken::from_near(0) {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        self.internal_storage_balance_of(&account_id).unwrap()
    }

    /// Method to match the interface of the storage deposit. Fails with a panic.
    #[payable]
    pub fn storage_withdraw(&mut self) {
        env::panic_str("Storage withdrawal is not supported");
    }

    /// Returns the minimum required balance to register an account.
    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: self.config.local_deposit,
            max: Some(self.config.local_deposit),
        }
    }

    /// Returns the minimum required balance to deploy a lockup.
    pub fn get_lockup_deployment_cost(&self) -> NearToken {
        self.config.min_lockup_deposit
    }
}
