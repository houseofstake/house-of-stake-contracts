use crate::venear::VenearGrowthConfig;
use crate::*;

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct GlobalState {
    pub update_timestamp: TimestampNs,

    pub total_venear_balance: VenearBalance,

    pub venear_growth_config: VenearGrowthConfig,
}

impl GlobalState {
    pub fn new(timestamp: TimestampNs, venear_growth_config: VenearGrowthConfig) -> Self {
        Self {
            update_timestamp: timestamp,
            total_venear_balance: VenearBalance::default(),
            venear_growth_config,
        }
    }

    pub fn update(&mut self, current_timestamp: TimestampNs) {
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
    Current(GlobalState),
}

impl From<GlobalState> for VGlobalState {
    fn from(global_state: GlobalState) -> Self {
        Self::Current(global_state)
    }
}

impl From<VGlobalState> for GlobalState {
    fn from(value: VGlobalState) -> Self {
        match value {
            VGlobalState::Current(global_state) => global_state,
        }
    }
}

impl VGlobalState {
    pub fn get_venear_growth_config(&self) -> &VenearGrowthConfig {
        match self {
            VGlobalState::Current(global_state) => &global_state.venear_growth_config,
        }
    }
}
