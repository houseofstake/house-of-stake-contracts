use crate::*;
use near_sdk::json_types::U64;

/// The timestamp in nanoseconds. It serializes as a string for JSON.
pub type TimestampNs = U64;

/// The version of the contract. It is a monotonically increasing number.
pub type Version = u64;

/// The balance and the timestamp when it was last updated.
/// Note: since most veNEAR balances are growing over time, we can't just store the balance, we need to
/// store the balance and the timestamp when it was last updated.
#[derive(Clone, Default)]
#[near(serializers=[borsh, json])]
pub struct TimedBalance {
    /// The balance at the time of the last update.
    pub balance: NearToken,

    /// The timestamp in nanoseconds when the balance was last updated.
    pub timestamp: TimestampNs,
}

/// The balance of fungible token with the token ID.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct FtBalance {
    /// The account ID of the fungible token contract.
    pub token_account_id: AccountId,

    /// The balance of the fungible token.
    pub balance: NearToken,
}
