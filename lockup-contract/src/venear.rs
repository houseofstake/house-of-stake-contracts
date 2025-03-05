use crate::venear_ext::{ext_venear, GAS_FOR_VENEAR_LOCKUP_UPDATE};
use common::lockup_update::{LockupUpdateV1, VLockupUpdate};
use near_sdk::{near, NearToken};

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
            .checked_sub(NearToken::from_yoctonear(self.venear_locked_balance))
            .expect("Illegal balance")
            .checked_sub(NearToken::from_yoctonear(self.venear_pending_balance))
            .expect("Illegal balance")
            .as_yoctonear()
    }

    fn set_venear_unlock_imestamp(&mut self) {
        self.venear_unlock_imestamp = 86400_000_000_000u64 * UNLOCK_PERIOD;
    }

    fn venear_lockup_update(&mut self) {
        self.lockup_update_nonce += 1;

        // Calls veNEAR with new total NEAR balance locked in the lockup
        ext_venear::ext(self.venear_account_id.clone())
            .with_static_gas(GAS_FOR_VENEAR_LOCKUP_UPDATE)
            .on_lockup_update(
                self.version,
                self.owner_account_id.clone(),
                VLockupUpdate::V1(LockupUpdateV1 {
                    locked_near_balance: NearToken::from_yoctonear(self.venear_locked_balance),
                    timestamp: env::block_timestamp().into(),
                    lockup_update_nonce: self.lockup_update_nonce,
                }),
            );
    }
}

#[near]
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

        self.venear_lockup_update();
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

        self.venear_lockup_update();
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

        self.venear_lockup_update();
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

        self.venear_lockup_update();
    }
}
