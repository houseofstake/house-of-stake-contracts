use crate::venear_ext::{ext_venear, GAS_FOR_VENEAR_LOCKUP_UPDATE};
use crate::*;
use common::lockup_update::{LockupUpdateV1, VLockupUpdate};
use common::{events, TimestampNs};
use near_sdk::json_types::U64;
use near_sdk::{assert_one_yocto, near, NearToken, Promise};

impl LockupContract {
    // get the amount of NEAR that is not locked in venear contract
    pub(crate) fn venear_liquid_balance(&self) -> Balance {
        // total available NEAR (including tokens on a lockup and staked in pools)
        let total_near_balance: u128 = env::account_balance().as_yoctonear()
            + self.get_known_deposited_balance().as_yoctonear();

        total_near_balance
            .checked_sub(self.venear_locked_balance)
            .expect("Illegal balance")
            .checked_sub(self.venear_pending_balance)
            .expect("Illegal balance")
    }

    fn set_venear_unlock_timestamp(&mut self) {
        self.venear_unlock_timestamp = env::block_timestamp() + self.unlock_duration_ns;
    }

    pub fn venear_lockup_update(&mut self) -> Promise {
        self.lockup_update_nonce += 1;

        // Calls veNEAR with new total NEAR balance locked in the lockup
        ext_venear::ext(self.venear_account_id.clone())
            .with_static_gas(GAS_FOR_VENEAR_LOCKUP_UPDATE)
            .on_lockup_update(
                self.version,
                self.owner_account_id.clone(),
                VLockupUpdate::V1(LockupUpdateV1 {
                    locked_near_balance: self.get_venear_locked_balance(),
                    timestamp: env::block_timestamp().into(),
                    lockup_update_nonce: U64::from(self.lockup_update_nonce),
                }),
            )
    }
}

#[near]
impl LockupContract {
    /// Returns the amount of NEAR locked in the lockup contract
    pub fn get_venear_locked_balance(&self) -> NearToken {
        NearToken::from_yoctonear(self.venear_locked_balance)
    }

    /// Returns the timestamp in nanoseconds when the pending amount will be unlocked
    pub fn get_venear_unlock_timestamp(&self) -> TimestampNs {
        self.venear_unlock_timestamp.into()
    }

    /// Returns the nonce of the lockup update
    pub fn get_lockup_update_nonce(&self) -> U64 {
        self.lockup_update_nonce.into()
    }

    /// Returns the amount of NEAR that is pending to be unlocked
    pub fn get_venear_pending_balance(&self) -> NearToken {
        NearToken::from_yoctonear(self.venear_pending_balance)
    }

    /// Returns the amount of NEAR that is liquid (the NEAR that can be locked)
    pub fn get_venear_liquid_balance(&self) -> NearToken {
        NearToken::from_yoctonear(self.venear_liquid_balance())
    }

    /// OWNER'S METHOD
    ///
    /// Requires 1 yoctoNEAR attached
    ///
    /// Locks the NEAR in the lockup contract.
    /// You can specify the amount of NEAR to lock, or if you don't specify it, all the liquid NEAR
    /// will be locked.
    #[payable]
    pub fn lock_near(&mut self, amount: Option<NearToken>) {
        self.assert_owner();
        assert_one_yocto();
        let amount: Balance = if let Some(amount) = amount {
            amount.as_yoctonear()
        } else {
            self.venear_liquid_balance()
        };

        assert!(amount <= self.venear_liquid_balance(), "Invalid amount");

        self.venear_locked_balance += amount;

        events::emit::lockup_action(
            "lockup_lock_near".as_ref(),
            &(env::current_account_id()),
            self.version,
            &Some(U64::from(self.lockup_update_nonce)),
            &Some(U64::from(env::block_timestamp())),
            &Some(NearToken::from_yoctonear(amount)),
        );

        self.venear_lockup_update();
    }

    /// OWNER'S METHOD
    ///
    /// Requires 1 yoctoNEAR attached
    ///
    /// Starts the unlocking process of the locked NEAR in the lockup contract.
    /// You specify the amount of near to unlock, or if you don't specify it, all the locked NEAR
    /// will be unlocked.
    /// (works similarly to unstaking from a staking pool).
    #[payable]
    pub fn begin_unlock_near(&mut self, amount: Option<NearToken>) {
        self.assert_owner();
        assert_one_yocto();
        let amount: Balance = if let Some(amount) = amount {
            amount.as_yoctonear()
        } else {
            self.venear_locked_balance
        };

        assert!(amount <= self.venear_locked_balance, "Invalid amount");

        self.venear_locked_balance -= amount;
        self.venear_pending_balance += amount;
        self.set_venear_unlock_timestamp();

        self.venear_lockup_update();
    }

    /// OWNER'S METHOD
    ///
    /// Requires 1 yoctoNEAR attached
    /// Requires that the unlock timestamp is reached
    ///
    /// Finishes the unlocking process of the NEAR in the lockup contract.
    /// You can specify the amount of NEAR to unlock, or if you don't specify it, all the pending
    /// NEAR will be unlocked.
    #[payable]
    pub fn end_unlock_near(&mut self, amount: Option<NearToken>) {
        self.assert_owner();
        assert_one_yocto();
        let amount: Balance = if let Some(amount) = amount {
            amount.as_yoctonear()
        } else {
            self.venear_pending_balance
        };

        assert!(amount <= self.venear_pending_balance, "Invalid amount");
        assert!(
            env::block_timestamp() >= self.venear_unlock_timestamp,
            "Invalid unlock time"
        );

        self.venear_pending_balance -= amount;

        self.venear_lockup_update();
    }

    /// OWNER'S METHOD
    ///
    /// Requires 1 yoctoNEAR attached
    ///
    /// Locks the pending NEAR in the lockup contract.
    /// You can specify the amount of NEAR to lock, or if you don't specify it, all the pending NEAR
    /// will be locked.
    #[payable]
    pub fn lock_pending_near(&mut self, amount: Option<NearToken>) {
        self.assert_owner();
        assert_one_yocto();
        let amount: Balance = if let Some(amount) = amount {
            amount.as_yoctonear()
        } else {
            self.venear_pending_balance
        };

        assert!(amount <= self.venear_pending_balance, "Invalid amount");

        self.venear_pending_balance -= amount;
        self.venear_locked_balance += amount;

        self.venear_lockup_update();
    }
}
