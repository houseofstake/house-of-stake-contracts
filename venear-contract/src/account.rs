use crate::*;
use common::{VenearBalance, Version};
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
    pub version: Option<Version>,

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
    /// Helper method to get the account info.
    pub fn get_account_info(&self, account_id: AccountId) -> Option<AccountInfo> {
        self.internal_get_account_internal(&account_id)
            .map(|internal| AccountInfo {
                account: self.internal_expect_account_updated(&account_id),
                internal,
            })
    }
}

impl Contract {
    pub fn internal_register_account(&mut self, account_id: &AccountId, deposit: NearToken) {
        require!(
            self.internal_set_account_internal(
                account_id.clone(),
                AccountInternal {
                    version: None,
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
        self.tree.set(account_id, account.into());
    }
}
