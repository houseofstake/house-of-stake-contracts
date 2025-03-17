use crate::*;

#[near]
impl Contract {
    #[private]
    #[init(ignore_state)]
    pub fn upgrade_state() -> Self {
        todo!();
    }

    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[no_mangle]
pub extern "C" fn upgrade() {
    env::setup_panic_hook();
    let contract: Contract = env::state_read().unwrap();
    contract.assert_owner();
    todo!();
}
