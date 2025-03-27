use crate::*;
use near_sdk::assert_one_yocto;

#[near]
impl Contract {
    /// Delegate all veNEAR tokens to the given receiver account ID.
    /// The receiver account ID must be registered in the contract.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn delegate_all(&mut self, receiver_id: AccountId) {
        assert_one_yocto();
        let predecessor_id = env::predecessor_account_id();
        require!(receiver_id != predecessor_id, "Can't delegate to self");
        let mut account = self.internal_expect_account_updated(&predecessor_id);
        if let Some(delegation) = &account.delegation {
            if receiver_id == delegation.account_id {
                return;
            }
        }
        if account.delegation.is_some() {
            self.internal_undelegate(&mut account);
        }

        let mut delegation_account = self.internal_expect_account_updated(&receiver_id);
        delegation_account.delegated_balance += account.balance;
        self.internal_set_account(receiver_id.clone(), delegation_account);

        account.delegation = Some(AccountDelegation {
            account_id: receiver_id,
        });
        self.internal_set_account(predecessor_id, account);
    }

    /// Undelegate all NEAR tokens.
    /// Requires 1 yocto NEAR.
    #[payable]
    pub fn undelegate(&mut self) {
        assert_one_yocto();
        let predecessor_id = env::predecessor_account_id();
        let mut account = self.internal_expect_account_updated(&predecessor_id);
        self.internal_undelegate(&mut account);
        self.internal_set_account(predecessor_id, account);
    }
}

impl Contract {
    pub fn internal_undelegate(&mut self, account: &mut Account) {
        let delegation_account_id = account.delegation.take().expect("Not delegated").account_id;
        let mut delegation_account = self.internal_expect_account_updated(&delegation_account_id);
        delegation_account.delegated_balance -= account.balance;
        self.internal_set_account(delegation_account_id, delegation_account);
    }
}
