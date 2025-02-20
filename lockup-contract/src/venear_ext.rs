use crate::*;
use near_sdk::{ext_contract, AccountId, NearToken};

pub const GAS_FOR_VENEAR_LOCKUP_UPDATE: Gas = Gas::from_tgas(20);

pub type TimestampNs = U64;

#[near(serializers=[json])]
pub struct LockupUpdate {
    /// The amount of NEAR that is locked in the lockup contract.
    pub locked_near_balance: NearToken,

    /// The timestamp in nanoseconds when the update was created.
    pub timestamp: TimestampNs,

    /// The nonce of the lockup update. It should be incremented for every new update by the lockup
    /// contract.
    pub lockup_update_nonce: u64,
}
#[ext_contract(ext_venear)]
trait ExtVenear {
    fn on_lockup_update(
        &mut self,
        version: Version,
        owner_account_id: AccountId,
        update: LockupUpdate,
    );
}
