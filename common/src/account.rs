use crate::*;

/// The account details that are stored in the Merkle Tree.
/// The current venear balance is calculated using the following:
/// venear_balance = lockup_near_balance * venear_grows_config.venear_grows / (now - update_timestamp) + venear_balance
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct Account {
    /// The account ID of the account. Required for the security of the Merkle Tree proofs.
    pub account_id: AccountId,
    /// The timestamp in nanoseconds when the account was last updated.
    pub update_timestamp: TimestampNs,
    /// The total NEAR balance of the account as reported by the lockup contract, including liquid
    /// staking tokens that were converted to NEAR amount at the time of the report.
    pub lockup_near_balance: NearToken,
    /// Additional veNEAR balance accumulated over time from growth.
    pub extra_venear_balance: NearToken,
    /// The amount of veNEAR that was delegated to this account.
    pub delegated_venear_balance: NearToken,
    /// The delegation details, in case this account has the account has delegated to another
    /// account.
    pub delegation: Option<AccountDelegation>,
}

/// The details of the delegation of veNEAR from one account to another.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct AccountDelegation {
    /// The account ID of the account that the veNEAR was delegated to.
    pub account_id: AccountId,

    /// The amount of veNEAR that was delegated to the account.
    pub venear_amount: TimedBalance,
}

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub enum VAccount {
    Current(Account),
}

impl From<Account> for VAccount {
    fn from(account: Account) -> Self {
        Self::Current(account)
    }
}

impl From<VAccount> for Account {
    fn from(value: VAccount) -> Self {
        match value {
            VAccount::Current(account) => account,
        }
    }
}
