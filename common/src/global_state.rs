use crate::*;

impl GlobalState {
    pub fn new() -> Self {
        Self {}
    }
}

impl From<GlobalState> for VGlobalState {
    fn from(global_state: GlobalState) -> Self {
        Self::Current(global_state)
    }
}

#[near(serializers=[borsh, json])]
pub enum VGlobalState {
    Current(GlobalState),
}

#[near(serializers=[borsh, json])]
pub struct GlobalState {
    // E.g. total sum
}
