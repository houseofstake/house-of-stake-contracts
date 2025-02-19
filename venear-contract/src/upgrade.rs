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

impl Contract {
    pub fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.config.owner_account_id,
            "Only owner can call this method"
        );
    }
}

#[no_mangle]
pub extern "C" fn upgrade() {
    env::setup_panic_hook();
    let contract: Contract = env::state_read().unwrap();
    contract.assert_owner();
    todo!();
}
