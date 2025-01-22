use crate::*;

#[derive(Clone)]
#[near(serializers=[json])]
pub struct AccountInfo {
    pub account_id: AccountId,
    // todo: add more fields
}

#[near]
impl Contract {
    #[payable]
    pub fn register_account(&mut self) {
        todo!()
    }

    pub fn get_registration_cost(&self) -> NearToken {
        todo!()
    }

    pub fn get_account_info(&self, account_id: AccountId) -> Option<AccountInfo> {
        todo!()
    }
}
