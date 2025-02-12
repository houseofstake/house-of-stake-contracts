use crate::*;
use near_sdk::json_types::{U128, U64};
use std::cmp::Ordering;
use std::ops::Mul;

uint::construct_uint!(
    pub struct U256(4);
);

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

#[derive(Clone, Copy)]
#[near(serializers=[borsh, json])]
pub struct Fraction {
    pub numerator: U128,
    pub denominator: U128,
}

impl PartialEq<Self> for Fraction {
    fn eq(&self, other: &Self) -> bool {
        U256::from(self.numerator.0) * U256::from(other.denominator.0)
            == U256::from(self.denominator.0) * U256::from(other.numerator.0)
    }
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (U256::from(self.numerator.0) * U256::from(other.denominator.0))
            .partial_cmp(&(U256::from(self.denominator.0) * U256::from(other.numerator.0)))
    }
}

impl Mul<u128> for Fraction {
    type Output = u128;

    fn mul(self, rhs: u128) -> Self::Output {
        let numerator = U256::from(self.numerator.0) * U256::from(rhs);
        let denominator = U256::from(self.denominator.0);
        (numerator / denominator).as_u128()
    }
}
