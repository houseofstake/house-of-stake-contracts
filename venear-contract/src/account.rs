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
}

#[near(serializers=[borsh])]
pub enum VAccountInternal {
    Current(AccountInternal),
}

impl From<AccountInternal> for VAccountInternal {
    fn from(account: AccountInternal) -> Self {
        Self::Current(account)
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
        todo!()
    }
}
