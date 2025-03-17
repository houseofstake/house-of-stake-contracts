use crate::*;

impl Contract {
    pub fn internal_global_state_updated(&self) -> GlobalState {
        let mut global_state: GlobalState = self.tree.get_global_state().clone().into();
        global_state.update(env::block_timestamp().into());
        global_state
    }

    pub fn internal_set_global_state(&mut self, global_state: GlobalState) {
        self.tree.set_global_state(global_state.into());
    }
}
