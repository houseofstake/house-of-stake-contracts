use crate::venear::VenearGrowthConfig;
use crate::*;
use near_sdk::json_types::{U128, U64};
use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

uint::construct_uint!(
    pub struct U256(4);
);

uint::construct_uint!(
    pub struct U384(6);
);

/// The timestamp in nanoseconds. It serializes as a string for JSON.
pub type TimestampNs = U64;

/// The version of the contract. It is a monotonically increasing number.
pub type Version = u64;

/// Represents balance of NEAR and veNEAR tokens. NEAR tokens grow over time, while veNEAR tokens
/// do not.
#[derive(Copy, Clone, Default)]
#[near(serializers=[borsh, json])]
pub struct VenearBalance {
    /// The balance in NEAR tokens. This balance grows over time.
    pub near_balance: NearToken,

    /// The balance in veNEAR tokens. This balance doesn't grow over time.
    pub extra_venear_balance: NearToken,
}

impl VenearBalance {
    pub fn total(&self) -> NearToken {
        near_add(self.near_balance, self.extra_venear_balance)
    }

    pub fn update(
        &mut self,
        previous_timestamp: TimestampNs,
        current_timestamp: TimestampNs,
        venear_growth_config: &VenearGrowthConfig,
    ) {
        self.extra_venear_balance = near_add(
            self.extra_venear_balance,
            venear_growth_config.calculate(
                previous_timestamp,
                current_timestamp,
                self.near_balance,
            ),
        );
    }

    pub fn from_near(near_balance: NearToken) -> Self {
        Self {
            near_balance,
            extra_venear_balance: NearToken::from_yoctonear(0),
        }
    }
}

impl Add<Self> for VenearBalance {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            near_balance: near_add(self.near_balance, rhs.near_balance),
            extra_venear_balance: near_add(self.extra_venear_balance, rhs.extra_venear_balance),
        }
    }
}

impl Sub<Self> for VenearBalance {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            near_balance: near_sub(self.near_balance, rhs.near_balance),
            extra_venear_balance: near_sub(self.extra_venear_balance, rhs.extra_venear_balance),
        }
    }
}

impl AddAssign<Self> for VenearBalance {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign<Self> for VenearBalance {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
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

impl Fraction {
    pub fn u384_mul(&self, a: u128, b: u128) -> u128 {
        let numerator = U384::from(self.numerator.0) * U384::from(a) * U384::from(b);
        let denominator = U384::from(self.denominator.0);
        (numerator / denominator).as_u128()
    }
}
