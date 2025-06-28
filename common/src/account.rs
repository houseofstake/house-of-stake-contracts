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
    pub delegated_balance: PooledVenearBalance,
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
    V0(Account),
}

impl From<Account> for VAccount {
    fn from(account: Account) -> Self {
        Self::V0(account)
    }
}

impl From<VAccount> for Account {
    fn from(value: VAccount) -> Self {
        match value {
            VAccount::V0(account) => account,
        }
    }
}

impl Account {
    /// Returns veNEAR balance of the account without modifications.
    pub fn total_balance(
        &self,
        current_timestamp: TimestampNs,
        venear_growth_config: &VenearGrowthConfig,
    ) -> NearToken {
        let current_timestamp = truncate_to_seconds(current_timestamp);
        require!(
            current_timestamp >= self.update_timestamp,
            "Timestamp must be increasing"
        );
        let mut delegated_balance = self.delegated_balance;
        delegated_balance.update(
            self.update_timestamp,
            current_timestamp,
            venear_growth_config,
        );
        let total = delegated_balance.total();
        if self.delegation.is_none() {
            let mut balance = self.balance;
            balance.update(
                self.update_timestamp,
                current_timestamp,
                venear_growth_config,
            );
            near_add(total, balance.total())
        } else {
            total
        }
    }

    pub fn update(
        &mut self,
        current_timestamp: TimestampNs,
        venear_growth_config: &VenearGrowthConfig,
    ) {
        let current_timestamp = truncate_to_seconds(current_timestamp);
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
