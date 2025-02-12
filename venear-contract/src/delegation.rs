use crate::*;
use near_sdk::assert_one_yocto;

#[near]
impl Contract {
    /// Delegate NEAR tokens to the given account.
    /// The amount is the total amount of NEAR tokens that will be delegated.
    /// None means all NEAR tokens that are available for delegation.
    /// Note, this method can be used to decrease the amount of NEAR tokens that are delegated.
    #[payable]
    pub fn delegate(&mut self, account_id: AccountId, amount: Option<NearToken>) {
        assert_one_yocto();
        todo!()
    }

    /// Undelegate all NEAR tokens.
    #[payable]
    pub fn undelegate(&mut self) {
        assert_one_yocto();
        todo!()
    }
}
