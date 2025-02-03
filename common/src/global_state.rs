use crate::venear::VenearGrowsConfig;
use crate::*;

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct GlobalState {
    pub total_venear_balance: TimedBalance,

    pub venear_grows_config: VenearGrowsConfig,
}

impl GlobalState {
    pub fn new(venear_grows_config: VenearGrowsConfig) -> Self {
        Self {
            total_venear_balance: TimedBalance::default(),
            venear_grows_config,
        }
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
