use crate::*;
use common::lockup_update::VLockupUpdate;

#[near]
impl Contract {
    /// Called by one of the lockup contracts to update the amount of
    /// NEAR and fungible tokens locked in the lockup contract .
    pub fn on_lockup_update(&mut self, update: VLockupUpdate) {
        todo!()
    }
}

/// Internal methods for the contract and lockup.
impl Contract {
    pub fn internal_deploy_lockup(&mut self, account_id: AccountId) {
        todo!()
    }
}
