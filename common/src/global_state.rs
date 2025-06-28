use crate::venear::VenearGrowthConfig;
use crate::*;

/// The global state of the veNEAR contract and the merkle tree.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct GlobalState {
    pub update_timestamp: TimestampNs,

    pub total_venear_balance: PooledVenearBalance,

    pub venear_growth_config: VenearGrowthConfig,
}

impl GlobalState {
    pub fn new(timestamp: TimestampNs, venear_growth_config: VenearGrowthConfig) -> Self {
        Self {
            update_timestamp: truncate_to_seconds(timestamp),
            total_venear_balance: PooledVenearBalance::default(),
            venear_growth_config,
        }
    }

    pub fn update(&mut self, current_timestamp: TimestampNs) {
        let current_timestamp = truncate_to_seconds(current_timestamp);
        self.total_venear_balance.update(
            self.update_timestamp,
            current_timestamp,
            &self.venear_growth_config,
        );
        self.update_timestamp = current_timestamp;
    }
}

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub enum VGlobalState {
    V0(GlobalState),
}

impl From<GlobalState> for VGlobalState {
    fn from(global_state: GlobalState) -> Self {
        Self::V0(global_state)
    }
}

impl From<VGlobalState> for GlobalState {
    fn from(value: VGlobalState) -> Self {
        match value {
            VGlobalState::V0(global_state) => global_state,
        }
    }
}

impl VGlobalState {
    pub fn get_venear_growth_config(&self) -> &VenearGrowthConfig {
        match self {
            VGlobalState::V0(global_state) => &global_state.venear_growth_config,
        }
    }
}
