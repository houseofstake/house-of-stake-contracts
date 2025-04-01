use crate::*;
use near_sdk::json_types::U64;
use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

/// Raw type for balance in yocto NEAR.
pub type Balance = u128;

/// Raw type for duration in nanoseconds
pub type Duration = u64;
/// Raw type for timestamp in nanoseconds
pub type Timestamp = u64;

/// Timestamp in nanosecond wrapped into a struct for JSON serialization as a string.
pub type WrappedTimestamp = U64;
/// Duration in nanosecond wrapped into a struct for JSON serialization as a string.
pub type WrappedDuration = U64;

/// Describes the status of transactions with the staking pool contract or terminated unvesting
/// amount withdrawal.
#[near(serializers=[borsh, json])]
pub enum TransactionStatus {
    /// There are no transactions in progress.
    Idle,
    /// There is a transaction in progress.
    Busy,
}

/// Contains information about current stake and delegation.
#[near(serializers=[borsh])]
pub struct StakingInformation {
    /// The Account ID of the staking pool contract.
    pub staking_pool_account_id: AccountId,

    /// Contains status whether there is a transaction in progress.
    pub status: TransactionStatus,

    /// The amount of tokens that were deposited from this account to the staking pool.
    /// NOTE: The unstaked amount on the staking pool might be higher due to staking rewards.
    pub deposit_amount: NearToken,
}
