use crate::*;
use near_sdk::json_types::U128;
use near_sdk::serde_json;
use near_sdk::{env, require};

#[cfg(target_arch = "wasm32")]
#[warn(unused_variables)]
#[no_mangle]
pub extern "C" fn ft_on_transfer() {
    env::setup_panic_hook();
    let contract: LockupContract = env::state_read().unwrap();
    require!(
        Some(&env::predecessor_account_id())
            == contract
                .staking_information
                .as_ref()
                .map(|s| &s.staking_pool_account_id),
        "Only currently selected LST is accepted"
    );

    env::value_return(&serde_json::to_vec(&U128::from(0u128)).unwrap())
}
