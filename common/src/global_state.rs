use crate::*;

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct GlobalState {
    pub total_venear_balance: TimedBalance,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            total_venear_balance: TimedBalance::default(),
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
