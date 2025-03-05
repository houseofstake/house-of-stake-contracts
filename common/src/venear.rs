use crate::*;
use near_sdk::require;

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub enum VenearGrowthConfig {
    FixedRate(Box<VenearGrowthConfigFixedRate>),
}

/// The fixed annual growth rate of veNEAR tokens.
/// Note, the growth rate can be changed in the future through the upgrade mechanism, by introducing
/// timepoints when the growth rate changes.
#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct VenearGrowthConfigFixedRate {
    /// The growth rate of veNEAR tokens per nanosecond. E.g. 6 / (NUM_SEC_IN_YEAR * 10**9) means
    /// 6% annual growth rate.
    pub annual_growth_rate_ns: Fraction,
}

impl From<VenearGrowthConfigFixedRate> for VenearGrowthConfig {
    fn from(config: VenearGrowthConfigFixedRate) -> Self {
        Self::FixedRate(Box::new(config))
    }
}

impl VenearGrowthConfig {
    pub fn calculate(
        &self,
        previous_timestamp: TimestampNs,
        current_timestamp: TimestampNs,
        balance: NearToken,
    ) -> NearToken {
        require!(
            current_timestamp >= previous_timestamp,
            "Timestamp must be increasing"
        );
        if previous_timestamp == current_timestamp {
            return NearToken::from_yoctonear(0);
        }
        match self {
            VenearGrowthConfig::FixedRate(config) => {
                let growth_period_ns = current_timestamp.0 - previous_timestamp.0;
                NearToken::from_yoctonear(
                    config
                        .annual_growth_rate_ns
                        .u384_mul(growth_period_ns as _, balance.as_yoctonear()),
                )
            }
        }
    }
}
