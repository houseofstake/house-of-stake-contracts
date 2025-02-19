use crate::venear::VenearGrowthConfig;
use crate::*;
use near_sdk::require;

/// The account details that are stored in the Merkle Tree.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct Account {
    /// The account ID of the account. Required for the security of the Merkle Tree proofs.
    pub account_id: AccountId,
    /// The timestamp in nanoseconds when the account was last updated.
    pub update_timestamp: TimestampNs,
    /// The total NEAR balance of the account as reported by the lockup contract and additional
    /// veNEAR accumulated over time.
    pub balance: VenearBalance,
    /// The total amount of NEAR and veNEAR that was delegated to this account.
    pub delegated_balance: VenearBalance,
    /// The delegation details, in case this account has delegated balance to another account.
    pub delegation: Option<AccountDelegation>,
}

/// The details of the delegation of veNEAR from one account to another.
/// In the first version we assume that the whole balance was delegated.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct AccountDelegation {
    /// The account ID of the account that the veNEAR was delegated to.
    pub account_id: AccountId,
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

impl Account {
    /// Returns veNEAR balance of the account without modifications.
    pub fn venear_balance(
        &self,
        current_timestamp: TimestampNs,
        venear_growth_config: &VenearGrowthConfig,
    ) -> NearToken {
        require!(
            current_timestamp >= self.update_timestamp,
            "Timestamp must be increasing"
        );
        let mut total = self.delegated_balance;
        if self.delegation.is_none() {
            total += self.balance;
        }
        total.update(
            self.update_timestamp,
            current_timestamp,
            venear_growth_config,
        );
        total.total()
    }

    pub fn update(
        &mut self,
        current_timestamp: TimestampNs,
        venear_growth_config: &VenearGrowthConfig,
    ) {
        require!(
            current_timestamp >= self.update_timestamp,
            "Timestamp must be increasing"
        );
        self.balance.update(
            self.update_timestamp,
            current_timestamp,
            venear_growth_config,
        );
        self.delegated_balance.update(
            self.update_timestamp,
            current_timestamp,
            venear_growth_config,
        );
        self.update_timestamp = current_timestamp;
    }
}
