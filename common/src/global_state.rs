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
#[derive(Clone)]
pub enum VGlobalState {
    Current(GlobalState),
}

#[near(serializers=[borsh, json])]
#[derive(Clone)]
pub struct GlobalState {
    // E.g. total sum
}
