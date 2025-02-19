use crate::*;

/// The lockup update is the information passed from the lockup contract to update veNEAR balances.
/// It includes the total amount of NEAR that is locked in the lockup contract and the list of
/// fungible tokens that are locked in the lockup contract.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct LockupUpdateV1 {
    /// The amount of NEAR that is locked in the lockup contract.
    pub locked_near_balance: NearToken,

    /// The timestamp in nanoseconds when the update was created.
    pub timestamp: TimestampNs,

    /// The nonce of the lockup update. It should be incremented for every new update by the lockup
    /// contract.
    pub lockup_update_nonce: u64,
}

#[near(serializers=[borsh, json])]
pub enum VLockupUpdate {
    V1(LockupUpdateV1),
}
