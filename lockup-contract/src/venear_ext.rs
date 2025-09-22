use crate::*;
use common::lockup_update::VLockupUpdate;
use near_sdk::{ext_contract, AccountId};

pub const GAS_FOR_VENEAR_LOCKUP_UPDATE: Gas = Gas::from_tgas(20);

#[allow(dead_code)]
#[ext_contract(ext_venear)]
trait ExtVenear {
    fn on_lockup_update(
        &mut self,
        version: Version,
        owner_account_id: AccountId,
        update: VLockupUpdate,
    );

    fn ft_on_transfer(&mut self, sender_id: String, amount: String, msg: String) -> String;
}
