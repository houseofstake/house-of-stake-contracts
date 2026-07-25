//! Utilities: share minting math, lock pricing helpers, and runtime clock wrappers.
//!
//! Time constants: [`NS_PER_DAY`] is `u128` for fixed-point price math; [`NS_PER_DAY_TIMESTAMP`] is the same
//! nanosecond length as `u64` for block timestamps (subscription billing anchors in `lock.rs`).
//!
//! Runtime clock wrappers provide mocked `block_timestamp` and `epoch_height` when compiled with
//! `feature = "test"`, falling back to native NEAR env values in production builds.

use crate::*;
use common::U256;
use near_sdk::env;

// =============================================================================
// Time constants
// =============================================================================

/// Fixed-point denominator for `Price.lock_factor_near_months`.
pub const LOCK_FACTOR_DENOM: u128 = 1_000_000_000_000_000_000_000_000;

/// Nanoseconds in one Gregorian day (`86400 * 10^9`), as `u128` (price lock and share math).
pub const NS_PER_DAY: u128 = 86_400_000_000_000;
/// Same interval as [`NS_PER_DAY`], as `u64`, for `u64` block timestamps (e.g. billing anchor day).
pub const NS_PER_DAY_TIMESTAMP: u64 = 86_400_000_000_000;
/// Average Gregorian month length in nanoseconds: `30.4375` days = `(487 / 16) * NS_PER_DAY`.
pub const AVG_MONTH_NS: u128 = NS_PER_DAY * 487 / 16;

// =============================================================================
// Share minting math
// =============================================================================

/// Mint shares for a new deposit. First deposit: 1:1 shares to yocto.
///
/// When `total_shares > 0`, callers must ensure `effective_total > 0` or rounding will mis-price mints
/// (see [`crate::validators::Validator::net_stake_yocto`] and guards in [`crate::lock`]).
pub fn mint_shares(total_shares: u128, effective_total: u128, deposit_yocto: u128) -> u128 {
    if total_shares == 0 || effective_total == 0 {
        return deposit_yocto;
    }
    let num = U256::from(deposit_yocto) * U256::from(total_shares);
    let den = U256::from(effective_total);
    (num / den).as_u128()
}

pub fn near_from_shares(shares: u128, effective_total: u128, total_shares: u128) -> u128 {
    if total_shares == 0 {
        return 0;
    }
    let num = U256::from(shares) * U256::from(effective_total);
    let den = U256::from(total_shares);
    (num / den).as_u128()
}

// =============================================================================
// Lock pricing helpers
// =============================================================================

/// Enforces `locked_yocto * duration_ns >= required_near_months * AVG_MONTH_NS`
/// where `required_near_months = amount * lock_factor / LOCK_FACTOR_DENOM`.
///
pub fn check_near_price_lock(
    price: &Price,
    locked_yocto: u128,
    duration_ns: u128,
) -> Result<(), &'static str> {
    let lhs = U256::from(locked_yocto) * U256::from(duration_ns);
    let rhs = required_near_months(price) * U256::from(AVG_MONTH_NS);
    if lhs >= rhs {
        Ok(())
    } else {
        Err("Locked NEAR or lock duration is too low for this catalog price")
    }
}

fn required_near_months(price: &Price) -> U256 {
    U256::from(price.amount.0) * U256::from(price.lock_factor_near_months.0)
        / U256::from(LOCK_FACTOR_DENOM)
}

// =============================================================================
// Runtime clock wrappers with test-feature storage overrides
// =============================================================================

// Storage keys for mocked values (test builds only)
pub const TEST_TIMESTAMP_KEY: &[u8] = b"_test_block_timestamp_";
pub const TEST_EPOCH_KEY: &[u8] = b"_test_epoch_height_";

/// Returns the current block timestamp.
#[cfg(not(feature = "test"))]
pub fn block_timestamp() -> u64 {
    env::block_timestamp()
}

/// Returns the current block timestamp (mocked in test builds).
#[cfg(feature = "test")]
pub fn block_timestamp() -> u64 {
    match env::storage_read(TEST_TIMESTAMP_KEY) {
        Some(raw) if raw.len() == 8 => {
            let bytes: [u8; 8] = raw.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_be_bytes(bytes)
        }
        _ => env::block_timestamp(),
    }
}

/// Returns the current epoch height.
#[cfg(not(feature = "test"))]
pub fn epoch_height() -> u64 {
    env::epoch_height()
}

/// Returns the current epoch height (mocked in test builds).
#[cfg(feature = "test")]
pub fn epoch_height() -> u64 {
    match env::storage_read(TEST_EPOCH_KEY) {
        Some(raw) if raw.len() == 8 => {
            let bytes: [u8; 8] = raw.as_slice().try_into().unwrap_or([0u8; 8]);
            u64::from_be_bytes(bytes)
        }
        _ => env::epoch_height(),
    }
}

// =============================================================================
// Contract utilities
// =============================================================================

impl Contract {
    pub(crate) fn collect_paginated<T, F>(
        &self,
        from_index: u64,
        limit: u64,
        total_len: u64,
        mut fetch: F,
    ) -> Vec<T>
    where
        F: FnMut(u32) -> Option<T>,
    {
        let mut out = Vec::new();
        let mut i = from_index;
        while i < total_len && (out.len() as u64) < limit {
            if let Some(item) = fetch(i as u32) {
                out.push(item);
            }
            i += 1;
        }
        out
    }
}

// =============================================================================
// Test-only methods for controlling mocked clock (only when feature = "test")
// =============================================================================

#[cfg(feature = "test")]
#[near]
impl Contract {
    /// Set a mocked block timestamp (nanoseconds since Unix epoch).
    /// Only available when compiled with `feature = "test"`.
    pub fn set_block_timestamp(&mut self, timestamp_ns: u64) {
        use near_sdk::env;
        let bytes = timestamp_ns.to_be_bytes();
        env::storage_write(TEST_TIMESTAMP_KEY, &bytes);
    }

    /// Read the currently mocked block timestamp, or actual env value if not set.
    /// Only available when compiled with `feature = "test"`.
    pub fn get_block_timestamp(&self) -> u64 {
        block_timestamp()
    }

    /// Set a mocked epoch height.
    /// Only available when compiled with `feature = "test"`.
    pub fn set_epoch_height(&mut self, epoch: u64) {
        use near_sdk::env;
        let bytes = epoch.to_be_bytes();
        env::storage_write(TEST_EPOCH_KEY, &bytes);
    }

    /// Read the currently mocked epoch height, or actual env value if not set.
    /// Only available when compiled with `feature = "test"`.
    pub fn get_epoch_height(&self) -> u64 {
        epoch_height()
    }

    /// Clear all mocked clock values, reverting to actual env values.
    /// Only available when compiled with `feature = "test"`.
    pub fn clear_test_clock(&mut self) {
        use near_sdk::env;
        env::storage_remove(TEST_TIMESTAMP_KEY);
        env::storage_remove(TEST_EPOCH_KEY);
    }

    /// Advance mocked timestamp by a delta (convenience for tests).
    /// Only available when compiled with `feature = "test"`.
    pub fn advance_block_timestamp(&mut self, delta_ns: u64) {
        let current = block_timestamp();
        self.set_block_timestamp(current.saturating_add(delta_ns));
    }

    /// Advance mocked epoch by a delta (convenience for tests).
    /// Only available when compiled with `feature = "test"`.
    pub fn advance_epoch_height(&mut self, delta: u64) {
        let current = epoch_height();
        self.set_epoch_height(current.saturating_add(delta));
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_first_deposit_one_to_one() {
        assert_eq!(
            mint_shares(0, 0, 1_000_000_000_000_000_000_000_000),
            1_000_000_000_000_000_000_000_000
        );
    }

    #[test]
    fn mint_pro_rata() {
        let s = mint_shares(2_000, 4_000, 1_000);
        assert_eq!(s, 500);
    }

    /// Two half-share exits in one epoch must not exceed one full exit (no double-count gross pool).
    #[test]
    fn sequential_share_exit_net_effective_totals_to_gross() {
        let gross = 100u128;
        let ts0 = 100u128;
        let sh = 50u128;
        let user_liab0 = 0u128;
        let eff1 = gross.saturating_sub(user_liab0);
        let n1 = near_from_shares(sh, eff1, ts0);
        let user_liab1 = user_liab0.saturating_add(n1);
        let ts1 = ts0.saturating_sub(sh);
        let eff2 = gross.saturating_sub(user_liab1);
        let n2 = near_from_shares(sh, eff2, ts1);
        assert_eq!(n1.saturating_add(n2), gross);
    }

    #[test]
    fn price_lock_check_does_not_saturate_required_near_months() {
        let one_near = 1_000_000_000_000_000_000_000_000;
        let one_month = AVG_MONTH_NS;
        let lock_factor = u128::MAX;

        let low_price = test_price(one_near, lock_factor);
        let high_price = test_price(one_near.saturating_mul(100), lock_factor);

        assert!(check_near_price_lock(&low_price, one_near, one_month).is_err());
        assert!(check_near_price_lock(&high_price, one_near, one_month).is_err());

        let low_required = required_near_months(&low_price);
        let high_required = required_near_months(&high_price);
        assert!(high_required > low_required);
    }

    fn test_price(amount: u128, lock_factor_near_months: u128) -> Price {
        use crate::types::{BillingPeriod, CatalogStatus, PriceType};
        use near_sdk::json_types::U128;

        Price {
            price_id: "price_x".into(),
            product_id: "prod_x".into(),
            name: "".into(),
            description: "".into(),
            amount: U128(amount),
            price_type: PriceType::Recurring,
            billing_period: Some(BillingPeriod::Monthly),
            lock_factor_near_months: U128(lock_factor_near_months),
            metadata: None,
            status: CatalogStatus::Active,
            usage_count: 0,
        }
    }
}
