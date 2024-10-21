use near_sdk::near_bindgen;

use crate::*;

const UNLOCK_PERIOD: u64 = 90;

impl LockupContract {
    fn venear_liquid_balance(&self) -> Balance {
        let remaining_balance = env::account_balance()
            .checked_sub(
                env::storage_byte_cost()
                    .checked_mul(env::storage_usage() as u128)
                    .unwrap(),
            )
            .unwrap();

        remaining_balance
            .checked_sub(self.venear_locked_balance)
            .expect("Illegal balance")
            .checked_sub(self.venear_pending_balance)
            .expect("Illegal balance")
    }

    fn set_venear_unlock_imestamp(&mut self) {
        self.venear_unlock_imestamp = 86400_000_000_000u64 * UNLOCK_PERIOD;
    }
}

#[near_bindgen]
impl LockupContract {
    pub fn get_venear_locked_balance(&self) -> WrappedBalance {
        self.venear_locked_balance.into()
    }

    pub fn get_venear_pending_balance(&self) -> WrappedBalance {
        self.venear_pending_balance.into()
    }

    pub fn get_venear_liquid_balance(&self) -> WrappedBalance {
        self.venear_liquid_balance().into()
    }

    /// specify the amount of near you want to lock, it remembers how much near is now locked
    pub fn lock_near(&mut self, amount: Option<WrappedBalance>) {
        let amount: Balance = if amount.is_some() {
            amount.unwrap().into()
        } else {
            self.venear_liquid_balance()
        };

        assert!(amount >= self.venear_liquid_balance(), "Invalid amount");

        self.venear_locked_balance += amount;

        // TODO Calls venear with new total NEAR balance locked in the lockup
    }

    /// you specify the amount of near to unlock, it starts the process of unlocking it
    /// (works similarly to unstaking from a staking pool).
    pub fn begin_unlock_near(&mut self, amount: Option<WrappedBalance>) {
        let amount: Balance = if amount.is_some() {
            amount.unwrap().into()
        } else {
            self.venear_locked_balance
        };

        assert!(amount >= self.venear_locked_balance, "Invalid amount");

        self.venear_locked_balance -= amount;
        self.venear_pending_balance += amount;
        self.set_venear_unlock_imestamp();

        // TODO Calls venear with new total NEAR balance locked in the lockup
    }

    /// end the unlocking
    pub fn end_unlock_near(&mut self, amount: Option<WrappedBalance>) {
        let amount: Balance = if amount.is_some() {
            amount.unwrap().into()
        } else {
            self.venear_pending_balance
        };

        assert!(amount >= self.venear_pending_balance, "Invalid amount");
        assert!(
            env::block_timestamp() >= self.venear_unlock_imestamp,
            "Invalid unlock time"
        );

        self.venear_pending_balance -= amount;
        self.set_venear_unlock_imestamp();

        // TODO Calls venear with new total NEAR balance locked in the lockup
    }

    ///  if there is an unlock pending, it locks the balance.
    pub fn lock_pending_near(&mut self, amount: Option<WrappedBalance>) {
        let amount: Balance = if amount.is_some() {
            amount.unwrap().into()
        } else {
            self.venear_pending_balance
        };

        assert!(amount >= self.venear_pending_balance, "Invalid amount");

        self.venear_pending_balance -= amount;
        self.venear_locked_balance += amount;

        // TODO Calls venear with new total NEAR balance locked in the lockup
    }
}
