//! A smart contract that allows tokens to be locked up.

pub use crate::getters::*;
pub use crate::owner::*;
pub use crate::owner_callbacks::*;
pub use crate::types::*;
pub use crate::venear::*;
use near_sdk::json_types::U64;
use near_sdk::{env, ext_contract, near, require, AccountId, PanicOnDefault};
use near_sdk::{Gas, NearToken};

pub mod gas;
pub mod owner_callbacks;
pub mod types;

pub mod getters;
pub mod internal;
pub mod owner;
pub mod venear;
pub mod venear_ext;

pub type Version = u64;

#[ext_contract(ext_staking_pool)]
pub trait ExtStakingPool {
    fn get_account_staked_balance(&self, account_id: AccountId) -> NearToken;

    fn get_account_unstaked_balance(&self, account_id: AccountId) -> NearToken;

    fn get_account_total_balance(&self, account_id: AccountId) -> NearToken;

    fn deposit(&mut self);

    fn deposit_and_stake(&mut self);

    fn withdraw(&mut self, amount: NearToken);

    fn stake(&mut self, amount: NearToken);

    fn unstake(&mut self, amount: NearToken);

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

    fn on_staking_pool_deposit(&mut self, amount: NearToken) -> bool;

    fn on_staking_pool_deposit_and_stake(&mut self, amount: NearToken) -> bool;

    fn on_staking_pool_withdraw(&mut self, amount: NearToken) -> bool;

    fn on_staking_pool_stake(&mut self, amount: NearToken) -> bool;

    fn on_staking_pool_unstake(&mut self, amount: NearToken) -> bool;

    fn on_staking_pool_unstake_all(&mut self) -> bool;

    fn on_get_result_from_transfer_poll(&mut self, #[callback] poll_result: PollResult) -> bool;

    fn on_get_account_total_balance(&mut self, #[callback] total_balance: NearToken);

    fn on_get_account_unstaked_balance_to_withdraw_by_owner(
        &mut self,
        #[callback] unstaked_balance: NearToken,
    );
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LockupContract {
    /// The account ID of the owner.
    pub owner_account_id: AccountId,

    /// Account Id of VeNEAR Contract
    pub venear_account_id: AccountId,

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

    /// The minimum amount in NEAR required for lockup deployment.
    pub min_lockup_deposit: NearToken,
}

#[near]
impl LockupContract {
    /// Requires 25 TGas (1 * BASE_GAS)
    ///
    /// Initializes lockup contract.
    /// - `owner_account_id` - the account ID of the owner. Only this account can call owner's
    ///    methods on this contract.
    /// - `venear_account_id` - the account ID of the VeNEAR contract.
    /// - `unlock_duration_ns` - The time in nanoseconds for unlocking the lockup amount.
    /// - `staking_pool_whitelist_account_id` - the Account ID of the staking pool whitelist contract.
    ///    The version of the contract. It is a monotonically increasing number.
    /// - `version` - Version of the lockup contract will be tracked by the veNEAR contract.
    /// - `lockup_update_nonce` - The nonce of the lockup update. It should be incremented for every
    ///   new update by the lockup contract.
    /// - `min_lockup_deposit` - The minimum amount in NEAR required for lockup deployment.
    #[payable]
    #[init]
    pub fn new(
        owner_account_id: AccountId,
        venear_account_id: AccountId,
        unlock_duration_ns: U64,
        staking_pool_whitelist_account_id: AccountId,
        version: Version,
        lockup_update_nonce: U64,
        min_lockup_deposit: NearToken,
    ) -> Self {
        require!(
            env::account_balance() >= min_lockup_deposit,
            "Not enough NEAR for storage"
        );
        Self {
            owner_account_id,
            venear_account_id,
            staking_information: None,
            staking_pool_whitelist_account_id,
            unlock_duration_ns: unlock_duration_ns.into(),
            venear_locked_balance: 0,
            venear_unlock_timestamp: 0u64,
            venear_pending_balance: 0,
            lockup_update_nonce: lockup_update_nonce.into(),
            version,
            min_lockup_deposit,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use near_sdk::{testing_env, AccountId, NearToken, VMContext};
    use std::convert::TryInto;
    use std::str::FromStr;
    use test_utils::*;
    use tests::assert_almost_eq;

    use super::*;

    mod test_utils;

    const VENEAR_ACCOUNT_ID: &str = "venear";
    const LOCKUP_VERSION: Version = 1;
    const UNLOCK_DURATION_NS: u64 = 90u64 * 24 * 60 * 60 * 10u64.pow(9);
    const MIN_LOCKUP_DEPOSIT: NearToken = NearToken::from_millinear(2000);

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
            MIN_LOCKUP_DEPOSIT,
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
            MIN_LOCKUP_DEPOSIT,
        );

        (context, contract)
    }

    #[test]
    fn test_lockup_only_basic() {
        let (mut context, contract) = lockup_only_setup();
        // Checking initial values at genesis time
        testing_env!(context.clone());

        assert_eq!(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));

        // Checking values in 1 day after genesis time
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + 1);

        assert_eq!(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));

        // Checking values next day after lockup timestamp
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR + 1);
        testing_env!(context.clone());

        assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));
    }

    #[test]
    #[should_panic(expected = "Can only be called by the owner")]
    fn test_call_by_non_owner() {
        let (mut context, mut contract) = lockup_only_setup();
        context.block_timestamp = to_ts(GENESIS_TIME_IN_DAYS + YEAR);
        context.predecessor_account_id = non_owner();
        context.signer_account_id = non_owner();
        context.attached_deposit = NearToken::from_yoctonear(1);
        testing_env!(context.clone());

        contract.select_staking_pool(AccountId::from_str("staking_pool").unwrap());
    }

    #[test]
    fn test_lockup_only_transfer_call_by_owner() {
        let (mut context, mut contract) = lockup_only_setup();

        assert_eq!(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));

        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(1).try_into().unwrap();
        testing_env!(context.clone());

        assert_eq!(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));

        context.attached_deposit = NearToken::from_yoctonear(1);
        testing_env!(context.clone());

        contract.transfer(NearToken::from_near(100), non_owner());
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
        context.attached_deposit = NearToken::from_yoctonear(1);

        let amount = to_yocto(LOCKUP_NEAR - 100);
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(NearToken::from_yoctonear(amount));
    }

    #[test]
    fn test_staking_pool_success() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();
        context.attached_deposit = NearToken::from_yoctonear(1);

        // Selecting staking pool
        let staking_pool: AccountId = AccountId::from_str("staking_pool").unwrap();
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        // context = clone_context(context.clone(), true);
        testing_env!(context.clone());
        assert_eq!(contract.get_staking_pool_account_id(), Some(staking_pool));
        assert_eq!(contract.get_known_deposited_balance().as_yoctonear(), 0);
        // context = clone_context(context.clone(), false);

        // Deposit to the staking_pool
        let amount = to_yocto(LOCKUP_NEAR - 100);
        context.account_balance = env::account_balance();
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.deposit_to_staking_pool(NearToken::from_yoctonear(amount));
        context.account_balance = env::account_balance();
        assert_almost_eq(
            context.account_balance.as_yoctonear(),
            to_yocto(LOCKUP_NEAR) - amount,
        );

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(NearToken::from_yoctonear(amount), true);
        // context = clone_context(context.clone(), true);
        testing_env!(context.clone());
        assert_eq!(
            contract.get_known_deposited_balance().as_yoctonear(),
            amount
        );

        // Staking on the staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.stake(NearToken::from_yoctonear(amount));

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_stake_inner(NearToken::from_yoctonear(amount), true);

        // Assuming there are 20 NEAR tokens in rewards. Unstaking.
        let unstake_amount = amount + to_yocto(20);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.unstake(NearToken::from_yoctonear(unstake_amount));

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_unstake_inner(NearToken::from_yoctonear(unstake_amount), true);

        // Withdrawing
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.withdraw_from_staking_pool(NearToken::from_yoctonear(unstake_amount));
        context.account_balance =
            NearToken::from_yoctonear(context.account_balance.as_yoctonear() + unstake_amount);

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_withdraw_inner(NearToken::from_yoctonear(unstake_amount), true);
        testing_env!(context.clone());
        assert_eq!(contract.get_known_deposited_balance().as_yoctonear(), 0);

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
        context.attached_deposit = NearToken::from_yoctonear(1);

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
        contract.deposit_to_staking_pool(NearToken::from_yoctonear(amount));
        context.account_balance = env::account_balance();
        assert_almost_eq(
            context.account_balance.as_yoctonear(),
            to_yocto(LOCKUP_NEAR) - amount,
        );

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(NearToken::from_yoctonear(amount), true);

        // Staking on the staking pool
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.stake(NearToken::from_yoctonear(amount));

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_stake_inner(NearToken::from_yoctonear(amount), true);

        testing_env!(context.clone());
        assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));
        assert_almost_eq(
            contract.get_liquid_owners_balance().as_yoctonear(),
            to_yocto(100) - MIN_LOCKUP_DEPOSIT.as_yoctonear(),
        );
        assert_eq!(
            contract.get_known_deposited_balance().as_yoctonear(),
            amount
        );

        // Assuming there are 20 NEAR tokens in rewards. Refreshing balance.
        let total_balance = amount + to_yocto(20);
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.refresh_staking_pool_balance();

        // In unit tests, the following call ignores the promise value, because it's passed directly.
        context.predecessor_account_id = lockup_account();
        contract.on_get_account_total_balance(NearToken::from_yoctonear(total_balance));

        testing_env!(context.clone());
        assert_eq!(
            contract.get_known_deposited_balance().as_yoctonear(),
            total_balance
        );
        assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1020));
        assert_almost_eq(
            contract.get_liquid_owners_balance().as_yoctonear(),
            to_yocto(100) - MIN_LOCKUP_DEPOSIT.as_yoctonear(),
        );

        // Withdrawing these tokens
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        let transfer_amount = to_yocto(15);
        contract.transfer(NearToken::from_yoctonear(transfer_amount), non_owner());
        context.account_balance = env::account_balance();

        testing_env!(context.clone());
        assert_eq!(
            contract.get_known_deposited_balance().as_yoctonear(),
            total_balance
        );
        assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1005));
        assert_almost_eq(
            contract.get_liquid_owners_balance().as_yoctonear(),
            to_yocto(100) - MIN_LOCKUP_DEPOSIT.as_yoctonear() - to_yocto(15),
        );
    }

    #[test]
    #[should_panic(expected = "Staking pool is already selected")]
    fn test_staking_pool_selected_again() {
        let (mut context, mut contract) = lockup_only_setup();
        context.predecessor_account_id = account_owner();
        context.signer_account_id = account_owner();
        context.signer_account_pk = public_key(2).try_into().unwrap();
        context.attached_deposit = NearToken::from_yoctonear(1);

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
        context.attached_deposit = NearToken::from_yoctonear(1);

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
        context.attached_deposit = NearToken::from_yoctonear(1);

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
        context.attached_deposit = NearToken::from_yoctonear(1);

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
        contract.deposit_to_staking_pool(NearToken::from_yoctonear(amount));
        context.account_balance = env::account_balance();

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_deposit_inner(NearToken::from_yoctonear(amount), true);

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
        assert_eq!(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));

        // Selecting staking pool
        let staking_pool = AccountId::from_str("staking_pool").unwrap();
        context.attached_deposit = NearToken::from_yoctonear(1);
        testing_env!(context.clone());
        contract.select_staking_pool(staking_pool.clone());

        context.predecessor_account_id = lockup_account();
        contract.on_whitelist_is_whitelisted(true, staking_pool.clone());

        assert_eq!(contract.get_known_deposited_balance().as_yoctonear(), 0);

        // Deposit to the staking_pool
        let mut total_amount = 0;
        let amount = to_yocto(100);
        for i in 1..=5 {
            total_amount += amount;
            context.predecessor_account_id = account_owner();
            testing_env!(context.clone());
            contract.deposit_to_staking_pool(NearToken::from_yoctonear(amount));
            context.account_balance = env::account_balance();
            assert_almost_eq(
                context.account_balance.as_yoctonear(),
                lockup_amount - total_amount,
            );

            context.predecessor_account_id = lockup_account();
            contract.on_staking_pool_deposit_inner(NearToken::from_yoctonear(amount), true);
            testing_env!(context.clone());
            assert_eq!(
                contract.get_known_deposited_balance().as_yoctonear(),
                total_amount
            );
            assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));
            assert_almost_eq(
                contract.get_liquid_owners_balance().as_yoctonear(),
                to_yocto(1000) - MIN_LOCKUP_DEPOSIT.as_yoctonear() - to_yocto(i * 100),
            );
        }

        // Withdrawing from the staking_pool.
        let mut total_withdrawn_amount = 0;
        for i in 1..=5 {
            total_withdrawn_amount += amount;
            context.predecessor_account_id = account_owner();
            testing_env!(context.clone());
            contract.withdraw_from_staking_pool(NearToken::from_yoctonear(amount));
            context.account_balance =
                NearToken::from_yoctonear(context.account_balance.as_yoctonear() + amount);
            assert_almost_eq(
                context.account_balance.as_yoctonear(),
                lockup_amount - total_amount + total_withdrawn_amount,
            );

            context.predecessor_account_id = lockup_account();
            contract.on_staking_pool_withdraw_inner(NearToken::from_yoctonear(amount), true);
            testing_env!(context.clone());
            assert_eq!(
                contract.get_known_deposited_balance().as_yoctonear(),
                total_amount.saturating_sub(total_withdrawn_amount)
            );
            assert_almost_eq(contract.get_owners_balance().as_yoctonear(), to_yocto(1000));
            assert_almost_eq(
                contract.get_liquid_owners_balance().as_yoctonear(),
                to_yocto(1000) - MIN_LOCKUP_DEPOSIT.as_yoctonear() - to_yocto((5 - i) * 100),
            )
        }

        // Withdrawing from the staking_pool one extra time as a reward
        context.predecessor_account_id = account_owner();
        testing_env!(context.clone());
        contract.withdraw_from_staking_pool(NearToken::from_yoctonear(amount));
        context.account_balance =
            NearToken::from_yoctonear(context.account_balance.as_yoctonear() + amount);

        context.predecessor_account_id = lockup_account();
        contract.on_staking_pool_withdraw_inner(NearToken::from_yoctonear(amount), true);
        testing_env!(context.clone());
        assert_eq!(
            contract.get_known_deposited_balance().as_yoctonear(),
            total_amount.saturating_sub(total_withdrawn_amount)
        );
        assert_almost_eq(
            contract.get_owners_balance().as_yoctonear(),
            to_yocto(1000) + amount,
        );
        assert_almost_eq(
            contract.get_liquid_owners_balance().as_yoctonear(),
            to_yocto(1000) + amount - MIN_LOCKUP_DEPOSIT.as_yoctonear(),
        );
    }
}
