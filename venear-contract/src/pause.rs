use crate::*;
use near_sdk::{assert_one_yocto, near, require};

#[near]
impl Contract {
    /// Checks if the contract is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pauses the contract.
    /// Can only be called by the guardian.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        self.assert_guardian();
        self.paused = true;
    }

    /// Unpauses the contract.
    /// Can only be called by the owner.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
    }
}

impl Contract {
    pub fn assert_not_paused(&self) {
        require!(!self.paused, "Contract is paused. Please try again later.");
    }
}
