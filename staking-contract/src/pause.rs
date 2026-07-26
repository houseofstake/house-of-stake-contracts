use crate::*;
use near_sdk::{assert_one_yocto, near, require};

#[near]
impl Contract {
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        self.assert_guardian();
        self.paused = true;
    }

    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
    }
}

impl Contract {
    pub fn assert_not_paused(&self) {
        require!(
            !self.paused,
            "The contract is paused; try again after it has been unpaused"
        );
    }
}
