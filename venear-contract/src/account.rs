use crate::*;
use common::{events, VenearBalance, Version};
use near_sdk::json_types::U64;

#[derive(Clone)]
#[near(serializers=[json])]
pub struct AccountInfo {
    /// Current account value from the Merkle tree.
    pub account: Account,

    /// Internal account information.
    pub internal: AccountInternal,
}

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct AccountInternal {
    /// The version of the lockup contract deployed. None means the lockup is not deployed.
    pub lockup_version: Option<Version>,

    /// The amount of NEAR tokens that are retained for the storage of the account.
    pub deposit: NearToken,

    /// The nonce of the last lockup update.
    pub lockup_update_nonce: U64,
}

#[derive(Clone)]
#[near(serializers=[borsh])]
pub enum VAccountInternal {
    Current(AccountInternal),
}

impl From<AccountInternal> for VAccountInternal {
    fn from(account: AccountInternal) -> Self {
        Self::Current(account)
    }
}

impl From<VAccountInternal> for AccountInternal {
    fn from(value: VAccountInternal) -> Self {
        match value {
            VAccountInternal::Current(account) => account,
        }
    }
}

#[near]
impl Contract {
    /// Returns the account info for a given account ID.
    pub fn get_account_info(&self, account_id: AccountId) -> Option<AccountInfo> {
        self.internal_get_account_internal(&account_id)
            .map(|internal| AccountInfo {
                account: self.internal_expect_account_updated(&account_id),
                internal,
            })
    }

    /// Returns the number of accounts.
    pub fn get_num_accounts(&self) -> u32 {
        self.tree.len() as u32
    }

    /// Returns the account info for a given index in the Merkle tree.
    pub fn get_account_by_index(&self, index: u32) -> Option<AccountInfo> {
        self.tree.get_by_index(index).map(|account| {
            let mut account: Account = account.clone().into();
            account.update(
                env::block_timestamp().into(),
                self.internal_get_venear_growth_config(),
            );
            let internal = self
                .internal_get_account_internal(&account.account_id)
                .unwrap();
            AccountInfo { account, internal }
        })
    }

    /// Returns a list of account info from the given index based on the merkle tree order.
    pub fn get_accounts(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AccountInfo> {
        let from_index = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(u32::MAX);
        let to_index = std::cmp::min(from_index.saturating_add(limit), self.get_num_accounts());
        (from_index..to_index)
            .into_iter()
            .filter_map(|i| self.get_account_by_index(i))
            .collect()
    }

    /// Returns a list of raw account data from the given index based on the merkle tree order.
    pub fn get_accounts_raw(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<&VAccount> {
        let from_index = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(u32::MAX);
        let to_index = std::cmp::min(from_index.saturating_add(limit), self.get_num_accounts());
        (from_index..to_index)
            .into_iter()
            .filter_map(|i| self.tree.get_by_index(i))
            .collect()
    }
}

impl Contract {
    pub fn internal_register_account(&mut self, account_id: &AccountId, deposit: NearToken) {
        require!(
            self.internal_set_account_internal(
                account_id.clone(),
                AccountInternal {
                    lockup_version: None,
                    deposit,
                    lockup_update_nonce: 0.into(),
                },
            )
            .is_none(),
            "Already registered"
        );
        let mut global_state: GlobalState = self.internal_global_state_updated();
        let account = Account {
            account_id: account_id.clone(),
            update_timestamp: env::block_timestamp().into(),
            balance: VenearBalance::from_near(deposit),
            delegated_balance: Default::default(),
            delegation: None,
        };
        global_state.total_venear_balance += account.balance;
        self.internal_set_account(account_id.clone(), account);
        self.internal_set_global_state(global_state);
    }

    pub fn internal_get_account_internal(&self, account_id: &AccountId) -> Option<AccountInternal> {
        self.accounts
            .get(account_id)
            .cloned()
            .map(|account| account.into())
    }

    pub fn internal_set_account_internal(
        &mut self,
        account_id: AccountId,
        account_internal: AccountInternal,
    ) -> Option<VAccountInternal> {
        self.accounts.insert(account_id, account_internal.into())
    }

    pub fn internal_get_account(&self, account_id: &AccountId) -> Option<Account> {
        self.tree
            .get(account_id)
            .cloned()
            .map(|account| account.into())
    }

    pub fn internal_expect_account_updated(&self, account_id: &AccountId) -> Account {
        let mut account = self
            .internal_get_account(account_id)
            .expect(format!("Account {} is not registered", account_id).as_str());
        account.update(
            env::block_timestamp().into(),
            self.internal_get_venear_growth_config(),
        );
        account
    }

    pub fn internal_set_account(&mut self, account_id: AccountId, account: Account) {
        // Previous balance
        let old_balance = self
            .internal_get_account(&account_id)
            .map(|account| {
                let mut total = account.delegated_balance;
                if account.delegation.is_none() {
                    total += account.balance;
                }
                total.total()
            })
            .unwrap_or_default();
        // New balance
        let mut new_balance = account.delegated_balance;
        if account.delegation.is_none() {
            new_balance += account.balance;
        }
        let new_balance = new_balance.total();
        if new_balance > old_balance {
            events::emit::ft_mint(&account_id, new_balance.checked_sub(old_balance).unwrap());
        } else if new_balance < old_balance {
            events::emit::ft_burn(&account_id, old_balance.checked_sub(new_balance).unwrap());
        }
        self.tree.set(account_id, account.into());
    }
}
