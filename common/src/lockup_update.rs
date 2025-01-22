use crate::*;

/// The lockup update is the information passed from the lockup contract to update veNEAR balances.
/// It includes the total amount of NEAR that is locked in the lockup contract and the list of
/// fungible tokens that are locked in the lockup contract.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct LockupUpdate {
    /// The amount of NEAR that is locked in the lockup contract.
    pub locked_near_balance: NearToken,

    /// The list of token ID and amount of liquid staking locked in the lockup contract.
    pub locked_fungible_tokens: Vec<FtBalance>,
}

#[near(serializers=[borsh, json])]
pub enum VLockupUpdate {
    Current(LockupUpdate),
}

impl From<LockupUpdate> for VLockupUpdate {
    fn from(lockup_update: LockupUpdate) -> Self {
        Self::Current(lockup_update)
    }
}
