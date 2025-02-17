use crate::*;
use common::Version;

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
    /// The version of the lockup contract.
    pub version: Version,

    /// The amount of NEAR tokens that are retained for the storage of the account.
    pub deposit: NearToken,

    /// The nonce of the last lockup update.
    pub lockup_update_nonce: u64,
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
    /// Registers a new account and attempts to deploy the lockup.
    #[payable]
    pub fn register_account(&mut self) {
        todo!()
    }

    /// The cost of registering a new account.
    pub fn get_registration_cost(&self) -> NearToken {
        todo!()
    }

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
    pub fn internal_get_account_internal(&self, account_id: &AccountId) -> Option<AccountInternal> {
        self.accounts
            .get(account_id)
            .cloned()
            .map(|account| account.into())
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
            .expect(format!("Account {} not found", account_id).as_str());
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
