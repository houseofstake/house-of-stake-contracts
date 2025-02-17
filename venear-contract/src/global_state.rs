use crate::*;
use common::TimestampNs;

impl Contract {
    pub fn internal_global_state_updated(&self) -> GlobalState {
        let mut global_state: GlobalState = self.tree.get_global_state().clone().into();
        let current_timestamp: TimestampNs = env::block_timestamp().into();
        global_state.total_venear_balance.update(
            global_state.update_timestamp,
            current_timestamp,
            &global_state.venear_growth_config,
        );
        global_state.update_timestamp = current_timestamp;
        global_state
    }

    pub fn internal_set_global_state(&mut self, global_state: GlobalState) {
        self.tree.set_global_state(global_state.into());
    }
}
