use crate::*;
use near_sdk::{ext_contract, AccountId};
use common::lockup_update::{VLockupUpdate};

pub const GAS_FOR_VENEAR_LOCKUP_UPDATE: Gas = Gas::from_tgas(20);

#[ext_contract(ext_venear)]
trait ExtVenear {
    fn on_lockup_update(
        &mut self,
        version: Version,
        owner_account_id: AccountId,
        update: VLockupUpdate,
    );
}
