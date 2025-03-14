//! A smart contract that allows tokens to be locked up.

pub use crate::getters::*;
pub use crate::owner::*;
pub use crate::owner_callbacks::*;
pub use crate::types::*;
pub use crate::venear::*;
use near_sdk::json_types::U64;
use near_sdk::Gas;
use near_sdk::{env, ext_contract, near, AccountId, PanicOnDefault};

pub mod gas;
pub mod owner_callbacks;
pub mod types;

pub mod getters;
pub mod internal;
pub mod owner;
pub mod venear;
pub mod venear_ext;

/// The contract keeps at least 1.6 NEAR in the account to avoid being transferred out to cover
/// contract code storage and some internal state.
pub const MIN_BALANCE_FOR_STORAGE: u128 = 1_600_000_000_000_000_000_000_000;

pub type Version = u64;

#[ext_contract(ext_staking_pool)]
pub trait ExtStakingPool {
    fn get_account_staked_balance(&self, account_id: AccountId) -> WrappedBalance;

    fn get_account_unstaked_balance(&self, account_id: AccountId) -> WrappedBalance;

    fn get_account_total_balance(&self, account_id: AccountId) -> WrappedBalance;

    fn deposit(&mut self);

    fn deposit_and_stake(&mut self);

    fn withdraw(&mut self, amount: WrappedBalance);

    fn stake(&mut self, amount: WrappedBalance);

    fn unstake(&mut self, amount: WrappedBalance);

    fn unstake_all(&mut self);
}

#[ext_contract(ext_whitelist)]
pub trait ExtStakingPoolWhitelist {
    fn is_whitelisted(&self, staking_pool_account_id: AccountId) -> bool;
}

#[ext_contract(ext_transfer_poll)]
pub trait ExtTransferPoll {
    fn get_result(&self) -> Option<PollResult>;
}

#[ext_contract(ext_self_owner)]
pub trait ExtLockupContractOwner {
    fn on_whitelist_is_whitelisted(
        &mut self,
        #[callback] is_whitelisted: bool,
        staking_pool_account_id: AccountId,
    ) -> bool;

    fn on_staking_pool_deposit(&mut self, amount: WrappedBalance) -> bool;

    fn on_staking_pool_deposit_and_stake(&mut self, amount: WrappedBalance) -> bool;

    fn on_staking_pool_withdraw(&mut self, amount: WrappedBalance) -> bool;

    fn on_staking_pool_stake(&mut self, amount: WrappedBalance) -> bool;

    fn on_staking_pool_unstake(&mut self, amount: WrappedBalance) -> bool;

    fn on_staking_pool_unstake_all(&mut self) -> bool;

    fn on_get_result_from_transfer_poll(&mut self, #[callback] poll_result: PollResult) -> bool;

    fn on_get_account_total_balance(&mut self, #[callback] total_balance: WrappedBalance);

    fn on_get_account_unstaked_balance_to_withdraw_by_owner(
        &mut self,
        #[callback] unstaked_balance: WrappedBalance,
    );
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LockupContract {
    /// The account ID of the owner.
    pub owner_account_id: AccountId,

    /// Account Id of VeNEAR Contract
    pub venear_account_id: AccountId,

    /// The amount in yocto-NEAR tokens locked for this account.
    pub lockup_amount: Balance,

    /// Account ID of the staking pool whitelist contract.
    pub staking_pool_whitelist_account_id: AccountId,

    /// Information about staking and delegation.
    /// `Some` means the staking information is available and the staking pool contract is selected.
    /// `None` means there is no staking pool selected.
    pub staking_information: Option<StakingInformation>,

    /// The time in nanoseconds for unlocking the lockup amount.
    pub unlock_duration_ns: u64,

    /// Locked amount
    pub venear_locked_balance: Balance,

    /// Timestamp to unlock
    pub venear_unlock_timestamp: Timestamp,

    /// Pending unlocking amount
    pub venear_pending_balance: Balance,

    /// The nonce of the lockup update. It should be incremented for every new update by the lockup
    /// contract.
    pub lockup_update_nonce: u64,

    /// Version of the lockup contract
    pub version: Version,
}

#[near]
impl LockupContract {
    /// Requires 25 TGas (1 * BASE_GAS)
    ///
    /// Initializes lockup contract.
    /// - `owner_account_id` - the account ID of the owner. Only this account can call owner's
    ///    methods on this contract.
    /// - `venear_account_id` - the account ID of the VeNEAR contract.
    /// - `staking_pool_whitelist_account_id` - the Account ID of the staking pool whitelist contract.
    ///    The version of the contract. It is a monotonically increasing number.
    /// - `version` - Version of the lockup contract will be tracked by the veNEAR contract.
    #[payable]
    #[init]
    pub fn new(
        owner_account_id: AccountId,
        venear_account_id: AccountId,
        unlock_duration_ns: U64,
        staking_pool_whitelist_account_id: AccountId,
        version: Version,
        lockup_update_nonce: U64,
    ) -> Self {
        Self {
            owner_account_id,
            venear_account_id,
            lockup_amount: env::account_balance().as_yoctonear(),
            staking_information: None,
            staking_pool_whitelist_account_id,
            unlock_duration_ns: unlock_duration_ns.into(),
            venear_locked_balance: 0,
            venear_unlock_timestamp: 0u64,
            venear_pending_balance: 0,
            lockup_update_nonce: lockup_update_nonce.into(),
            version,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use near_sdk::json_types::U64;
    use near_sdk::{testing_env, AccountId, NearToken, VMContext};
    use std::convert::TryInto;
    use std::str::FromStr;
    use test_utils::*;

    use super::*;

    mod test_utils;

    const VENEAR_ACCOUNT_ID: &str = "venear";
    const LOCKUP_VERSION: Version = 1;
    const UNLOCK_DURATION_NS: u64 = 90u64 * 24 * 60 * 60 * 10u64.pow(9);

    fn basic_context() -> VMContext {
        get_context(
            system_account(),
            to_yocto(LOCKUP_NEAR),
            0,
            to_ts(GENESIS_TIME_IN_DAYS),
        )
    }

    fn new_contract(
        transfers_enabled: bool,
        release_duration: Option<WrappedDuration>,
    ) -> LockupContract {
        LockupContract::new(
            account_owner(),
            AccountId::from_str(VENEAR_ACCOUNT_ID).unwrap(),
            UNLOCK_DURATION_NS.into(),
            AccountId::from_str("whitelist").unwrap(),
            LOCKUP_VERSION,
            0.into(),
        )
    }

    fn lockup_only_setup() -> (VMContext, LockupContract) {
        let context = basic_context();
        testing_env!(context.clone());

        let contract = LockupContract::new(
            account_owner(),
            AccountId::from_str(VENEAR_ACCOUNT_ID).unwrap(),
            UNLOCK_DURATION_NS.into(),
            AccountId::from_str("whitelist").unwrap(),
            LOCKUP_VERSION,
            0.into(),
        );

        (context, contract)
    }

    #[test]
    fn test_lockup_only_basic() {
        let (mut context, contract) = lockup_only_setup();
        // Checking initial values at genesis time
        testing_env!(context.clone());

        assert_eq!(contract.get_owners_balance().0, 0);

        // Checking values in 1 day after genesis time
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + 1);

        assert_eq!(contract.get_owners_balance().0, 0);

        // Checking values next day after lockup timestamp
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR + 1);
        testing_env!(context.clone());

        assert_almost_eq(contract.get_owners_balance().0, 0);
    }

    #[test]
    #[should_panic(expected = "Tokens are still locked/unvested")]
    fn test_add_full_access_key() {
        let (mut context, mut contract) = lockup_only_setup();
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR);
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(1).try_into().unwrap();
        testing_env!(context.clone());

        contract.add_full_access_key(public_key(4));
    }

    #[test]
    #[should_panic(expected = "Tokens are still locked/unvested")]
    fn test_add_full_access_key_when_vesting_is_not_finished() {
        let mut context = basic_context();
        testing_env!(context.clone());
        let mut contract = new_contract(true, None);

        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR - 10);
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(1).try_into().unwrap();
        testing_env!(context.clone());

        contract.add_full_access_key(public_key(4));
    }

    #[test]
    #[should_panic(expected = "Tokens are still locked/unvested")]
    fn test_add_full_access_key_when_lockup_is_not_finished() {
        let mut context = basic_context();
        testing_env!(context.clone());
        let mut contract = new_contract(true, Some(to_nanos(YEAR).into()));

        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR - 10);
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(1).try_into().unwrap();
        testing_env!(context.clone());

        contract.add_full_access_key(public_key(4));
    }

    #[test]
    #[should_panic(expected = "Can only be called by the owner")]
    fn test_call_by_non_owner() {
        let (mut context, mut contract) = lockup_only_setup();
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR);
        context.predecessor_account_id = non_owner();
        context.signer_account_id = non_owner();
        testing_env!(context.clone());

        contract.select_staking_pool(AccountId::from_str("staking_pool").unwrap());
    }

    #[test]
    #[should_panic(
        expected = "The available liquid balance 0 is smaller than the requested transfer amount 100000000000000000000000000"
    )]
    fn test_lockup_only_transfer_call_by_owner() {
        let (mut context, mut contract) = lockup_only_setup();

        assert_eq!(contract.get_owners_balance().0, 0);

        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(1).try_into().unwrap();
        testing_env!(context.clone());

        assert_eq!(contract.get_owners_balance().0, 0);
        contract.transfer(to_yocto(100).into(), non_owner());
        assert_almost_eq(
            env::account_balance().as_yoctonear(),
            to_yocto(LOCKUP_NEAR - 100),
        );
    }

    #[test]
    #[should_panic(expected = "Staking pool is not selected")]
    fn test_staking_pool_is_not_selected() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        let amount = to_yocto(LOCKUP_NEAR - 100);
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(amount.into());
    }

    #[test]
    fn test_staking_pool_success() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Selecting staking pool
        let staking_pool: AccountId = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        // context = clone_context(context.clone(), true);
        testing_env!(context.clone());
        assert_eq!(contract.get_staking_pool_account_id(), Some(staking_pool));
        assert_eq!(contract.get_known_deposited_balance().0, 0);
        // context = clone_context(context.clone(), false);

        // Deposit to the staking_pool
        let amount = to_yocto(LOCKUP_NEAR - 100);
        context.account_balance = env::account_balance();
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(amount.into());
        context.account_balance = env::account_balance();
        assert_eq!(
            context.account_balance.as_yoctonear(),
            to_yocto(LOCKUP_NEAR) - amount
        );

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(amount.into(), true);
        // context = clone_context(context.clone(), true);
        testing_env!(context.clone());
        assert_eq!(contract.get_known_deposited_balance().0, amount);

        // Staking on the staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.stake(amount.into());

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_stake_inner(amount.into(), true);

        // Assuming there are 20 NEAR tokens in rewards. Unstaking.
        let unstake_amount = amount + to_yocto(20);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.unstake(unstake_amount.into());

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_unstake_inner(unstake_amount.into(), true);

        // Withdrawing
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.withdraw_from_staking_pool(unstake_amount.into());
        context.account_balance =
            NearToken::from_yoctonear(context.account_balance.as_yoctonear() + unstake_amount);

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_withdraw_inner(unstake_amount.into(), true);
        testing_env!(context.clone());
        assert_eq!(contract.get_known_deposited_balance().0, 0);

        // Unselecting staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.unselect_staking_pool();
        assert_eq!(contract.get_staking_pool_account_id(), None);
    }

    #[test]
    fn test_staking_pool_refresh_balance() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Selecting staking pool
        let staking_pool: AccountId = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        // Deposit to the staking_pool
        let amount = to_yocto(LOCKUP_NEAR - 100);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(amount.into());
        context.account_balance = env::account_balance();
        assert_eq!(
            context.account_balance.as_yoctonear(),
            to_yocto(LOCKUP_NEAR) - amount
        );

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(amount.into(), true);

        // Staking on the staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.stake(amount.into());

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_stake_inner(amount.into(), true);

        testing_env!(context.clone());
        assert_eq!(contract.get_owners_balance().0, 0);
        assert_eq!(contract.get_liquid_owners_balance().0, 0);
        assert_eq!(contract.get_known_deposited_balance().0, amount);

        // Assuming there are 20 NEAR tokens in rewards. Refreshing balance.
        let total_balance = amount + to_yocto(20);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.refresh_staking_pool_balance();

        // In unit tests, the following call ignores the promise value, because it's passed directly.
        context.predecessor_account_id = lockup_account();
        contract.on_get_account_total_balance(total_balance.into());

        testing_env!(context.clone());
        assert_eq!(contract.get_known_deposited_balance().0, total_balance);
        assert_eq!(contract.get_owners_balance().0, to_yocto(20));
        assert_eq!(contract.get_liquid_owners_balance().0, to_yocto(20));

        // Withdrawing these tokens
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        let transfer_amount = to_yocto(15);
        contract.transfer(transfer_amount.into(), non_owner());
        context.account_balance = env::account_balance();

        testing_env!(context.clone());
        assert_eq!(contract.get_known_deposited_balance().0, total_balance);
        assert_eq!(contract.get_owners_balance().0, to_yocto(5));
        assert_eq!(contract.get_liquid_owners_balance().0, to_yocto(5));
    }

    #[test]
    #[should_panic(expected = "Staking pool is already selected")]
    fn test_staking_pool_selected_again() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Selecting staking pool
        let staking_pool = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        // Selecting another staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.select_staking_pool(AccountId::from_str("staking_pool_2").unwrap());
    }

    #[test]
    #[should_panic(expected = "The given staking pool account ID is not whitelisted")]
    fn test_staking_pool_not_whitelisted() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Selecting staking pool
        let staking_pool: AccountId = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(false, staking_pool.clone());
    }

    #[test]
    #[should_panic(expected = "Staking pool is not selected")]
    fn test_staking_pool_unselecting_non_selected() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Unselecting staking pool
        testing_env!(context.clone());
        contract.unselect_staking_pool();
    }

    #[test]
    #[should_panic(expected = "There is still a deposit on the staking pool")]
    fn test_staking_pool_unselecting_with_deposit() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        // Selecting staking pool
        let staking_pool = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        // Deposit to the staking_pool
        let amount = to_yocto(LOCKUP_NEAR - 100);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(amount.into());
        context.account_balance = env::account_balance();

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(amount.into(), true);

        // Unselecting staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.unselect_staking_pool();
    }

    #[test]
    fn test_staking_pool_owner_balance() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();

        let lockup_amount = to_yocto(LOCKUP_NEAR);
        testing_env!(context.clone());
        assert_eq!(contract.get_owners_balance().0, 0);

        // Selecting staking pool
        let staking_pool = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        assert_eq!(contract.get_known_deposited_balance().0, 0);

        // Deposit to the staking_pool
        let mut total_amount = 0;
        let amount = to_yocto(100);
        for _ in 1..=5 {
            total_amount += amount;
            context.predecessor_account_id = account_owner();
            testing_env!(context.clone());
            contract.deposit_to_staking_pool(amount.into());
            context.account_balance = env::account_balance();
            assert_eq!(
                context.account_balance.as_yoctonear(),
                lockup_amount - total_amount
            );

            context.predecessor_account_id = lockup_account();
            contract.on_staking_pool_deposit_inner(amount.into(), true);
            testing_env!(context.clone());
            assert_eq!(contract.get_known_deposited_balance().0, total_amount);
            assert_eq!(contract.get_owners_balance().0, 0);
            assert_eq!(contract.get_liquid_owners_balance().0, 0);
        }

        // Withdrawing from the staking_pool.
        let mut total_withdrawn_amount = 0;
        for _ in 1..=5 {
            total_withdrawn_amount += amount;
            context.predecessor_account_id = account_owner();
            testing_env!(context.clone());
            contract.withdraw_from_staking_pool(amount.into());
            context.account_balance =
                NearToken::from_yoctonear(context.account_balance.as_yoctonear() + amount);
            assert_eq!(
                context.account_balance.as_yoctonear(),
                lockup_amount - total_amount + total_withdrawn_amount
            );

            context.predecessor_account_id = lockup_account();
            contract.on_staking_pool_withdraw_inner(amount.into(), true);
            testing_env!(context.clone());
            assert_eq!(
                contract.get_known_deposited_balance().0,
                total_amount.saturating_sub(total_withdrawn_amount)
            );
            assert_eq!(contract.get_owners_balance().0, 0);
            assert_eq!(contract.get_liquid_owners_balance().0, 0);
        }

        // Withdrawing from the staking_pool one extra time as a reward
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.withdraw_from_staking_pool(amount.into());
        context.account_balance =
            NearToken::from_yoctonear(context.account_balance.as_yoctonear() + amount);

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_withdraw_inner(amount.into(), true);
        testing_env!(context.clone());
        assert_eq!(
            contract.get_known_deposited_balance().0,
            total_amount.saturating_sub(total_withdrawn_amount)
        );
        assert_eq!(contract.get_owners_balance().0, amount);
        assert_eq!(contract.get_liquid_owners_balance().0, amount);
    }
}
