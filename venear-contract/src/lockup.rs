use crate::account::AccountInternal;
use crate::config::LockupContractConfig;
use crate::*;
use common::lockup_update::{LockupUpdateV1, VLockupUpdate};
use common::near_add;
use common::{events, near_sub};
use near_sdk::json_types::{Base58CryptoHash, U64};
use near_sdk::{env, is_promise_success, Gas, IntoStorageKey, Promise};

const LOCKUP_DEPLOY_MIN_GAS: Gas = Gas::from_tgas(20);
const ON_LOCKUP_DEPLOYED: Gas = Gas::from_tgas(15);

#[near(serializers=[json])]
pub struct LockupInitArgs {
    version: Version,

    owner_account_id: AccountId,
    venear_account_id: AccountId,

    unlock_duration_ns: U64,
    staking_pool_whitelist_account_id: AccountId,

    /// Starting nonce for lockup updates. It should be unique for every lockup contract.
    lockup_update_nonce: U64,

    min_lockup_deposit: NearToken,
}

#[near(serializers=[json])]
pub struct OnLockupDeployedArgs {
    version: Version,

    account_id: AccountId,

    lockup_update_nonce: U64,

    lockup_deposit: NearToken,
}

#[near]
impl Contract {
    /// Deploys the lockup contract.
    /// If the lockup contract is already deployed, the method will fail after the attempt.
    /// Requires the caller to attach the deposit for the lockup contract of at least
    /// `get_lockup_deployment_cost()`.
    /// Requires the caller to already be registered.
    #[payable]
    pub fn deploy_lockup(&mut self) {
        self.assert_not_paused();
        self.internal_deploy_lockup(env::predecessor_account_id());
    }

    /// Called by one of the lockup contracts to update the amount of NEAR locked in the lockup
    /// contract.
    pub fn on_lockup_update(
        &mut self,
        version: Version,
        owner_account_id: AccountId,
        update: VLockupUpdate,
    ) {
        let lockup_account_id = self.get_lockup_account_id(&owner_account_id);
        require!(
            env::predecessor_account_id() == lockup_account_id,
            "Permission denied"
        );
        let account_internal = self
            .internal_get_account_internal(&owner_account_id)
            .expect("Account not found");
        require!(
            account_internal.lockup_version == Some(version),
            "Invalid lockup version"
        );

        match update {
            VLockupUpdate::V1(lockup_update) => {
                events::emit::lockup_action(
                    "lockup_update",
                    &owner_account_id,
                    version,
                    &Some(lockup_update.lockup_update_nonce),
                    &Some(lockup_update.timestamp),
                    &Some(lockup_update.locked_near_balance),
                );
                self.internal_lockup_update(owner_account_id, account_internal, lockup_update);
            }
        }
    }

    /// Callback after the attempt to deploy the lockup contract.
    /// Returns the lockup contract account ID if the deployment was successful.
    #[private]
    pub fn on_lockup_deployed(
        &mut self,
        version: Version,
        account_id: AccountId,
        lockup_update_nonce: U64,
        lockup_deposit: NearToken,
    ) -> Option<AccountId> {
        if is_promise_success() {
            let mut account_internal = self
                .internal_get_account_internal(&account_id)
                .expect("Account not found");
            account_internal.lockup_version = Some(version);
            require!(
                account_internal.lockup_update_nonce <= lockup_update_nonce,
                "Invalid nonce"
            );
            account_internal.lockup_update_nonce = lockup_update_nonce;

            events::emit::lockup_action(
                "lockup_deployed",
                &account_id,
                version,
                &None,
                &None,
                &None,
            );

            self.internal_set_account_internal(account_id.clone(), account_internal);

            Some(self.get_lockup_account_id(&account_id))
        } else {
            // Refunding the deposit if the lockup contract deployment failed.
            Promise::new(account_id).transfer(lockup_deposit);
            None
        }
    }

    /// Returns the account ID for the lockup contract for the given account.
    /// Note, the lockup contract is not guaranteed to be deployed.
    pub fn get_lockup_account_id(&self, account_id: &AccountId) -> AccountId {
        let owner_account_id_hash = hex::encode(&env::sha256(account_id.as_bytes())[0..20]);
        format!("{}.{}", owner_account_id_hash, env::current_account_id())
            .try_into()
            .expect("Failed to create lockup account ID")
    }
}

/// Internal methods for the contract and lockup.
impl Contract {
    pub fn internal_lockup_update(
        &mut self,
        account_id: AccountId,
        mut account_internal: AccountInternal,
        lockup_update: LockupUpdateV1,
    ) {
        require!(
            lockup_update.lockup_update_nonce > account_internal.lockup_update_nonce,
            "Invalid nonce"
        );
        account_internal.lockup_update_nonce = lockup_update.lockup_update_nonce;

        let mut account: Account = self.internal_expect_account_updated(&account_id);
        let old_balance = account.balance;
        let mut global_state: GlobalState = self.internal_global_state_updated();
        // Decreasing the locked NEAR will result in dropped extra veNEAR rewards.
        if lockup_update.locked_near_balance < old_balance.near_balance {
            account.balance.extra_venear_balance = NearToken::from_yoctonear(0);
        }
        // Updating balance and also adding internal balance deposit.
        account.balance.near_balance =
            near_add(lockup_update.locked_near_balance, account_internal.deposit);
        global_state.total_venear_balance -= old_balance;
        global_state.total_venear_balance += account.balance;

        if let Some(delegation) = &account.delegation {
            let mut delegation_account =
                self.internal_expect_account_updated(&delegation.account_id);
            delegation_account.delegated_balance -= old_balance;
            delegation_account.delegated_balance += account.balance;
            self.internal_set_account(delegation.account_id.clone(), delegation_account);
        }
        self.internal_set_account_internal(account_id.clone(), account_internal);
        self.internal_set_account(account_id, account);
        self.internal_set_global_state(global_state);
    }

    pub fn internal_set_lockup(&mut self, contract_hash: CryptoHash) {
        // read contract length
        let key = StorageKeys::LockupCode(contract_hash).into_storage_key();
        const CONTRACT_REGISTER: u64 = 0;
        let (size, hash) = match unsafe {
            sys::storage_read(key.len() as _, key.as_ptr() as _, CONTRACT_REGISTER)
        } {
            0 => env::panic_str("Contract hash is not found"),
            1 => internal_get_hash_and_size(CONTRACT_REGISTER),
            _ => env::abort(),
        };
        require!(hash == contract_hash);
        self.config.lockup_contract_config = Some(LockupContractConfig {
            contract_size: size as _,
            contract_version: self
                .config
                .lockup_contract_config
                .as_ref()
                .map(|c| c.contract_version)
                .unwrap_or(0)
                + 1,
            contract_hash: contract_hash.into(),
        });
    }

    pub fn internal_deploy_lockup(&mut self, owner_account_id: AccountId) {
        let lockup_deposit = env::attached_deposit();
        assert!(
            self.internal_get_account_internal(&owner_account_id)
                .is_some(),
            "Account {} is not registered",
            owner_account_id
        );
        let required_deposit = self.get_lockup_deployment_cost();
        assert!(
            lockup_deposit >= required_deposit,
            "Not enough deposit. Required: {}",
            required_deposit
        );
        let lockup_contract_config = self
            .config
            .lockup_contract_config
            .as_ref()
            .expect("The lockup contract code is not initialized");
        let lockup_account_id = self.get_lockup_account_id(&owner_account_id);
        let lockup_account_id = lockup_account_id.as_str();
        let contract_code_key =
            StorageKeys::LockupCode(lockup_contract_config.contract_hash.into()).into_storage_key();
        const CONTRACT_REGISTER: u64 = 0;
        let res = unsafe {
            sys::storage_read(
                contract_code_key.len() as _,
                contract_code_key.as_ptr() as _,
                CONTRACT_REGISTER,
            )
        };
        // Safety check
        require!(res == 1);

        let promise_id = unsafe {
            sys::promise_batch_create(
                lockup_account_id.len() as _,
                lockup_account_id.as_ptr() as _,
            )
        };
        let method_name = b"new";
        let lockup_update_nonce = env::block_height() * 1_000_000;
        let arguments = LockupInitArgs {
            version: lockup_contract_config.contract_version,
            owner_account_id: owner_account_id.clone(),
            venear_account_id: env::current_account_id(),
            unlock_duration_ns: self.config.unlock_duration_ns.clone(),
            staking_pool_whitelist_account_id: self
                .config
                .staking_pool_whitelist_account_id
                .clone(),
            lockup_update_nonce: lockup_update_nonce.into(),
            min_lockup_deposit: self.config.min_lockup_deposit,
        };
        let arguments =
            serde_json::to_vec(&arguments).expect("Failed to serialize lockup init args");
        unsafe {
            sys::promise_batch_action_create_account(promise_id);
            sys::promise_batch_action_deploy_contract(promise_id, u64::MAX, CONTRACT_REGISTER);
            sys::promise_batch_action_function_call_weight(
                promise_id,
                method_name.len() as _,
                method_name.as_ptr() as _,
                arguments.len() as _,
                arguments.as_ptr() as _,
                &lockup_deposit.as_yoctonear() as *const u128 as _,
                LOCKUP_DEPLOY_MIN_GAS.as_gas(),
                1,
            );
        }
        let current_account_id = env::current_account_id();
        let current_account_id = current_account_id.as_str();
        let method_name = b"on_lockup_deployed";
        let arguments = OnLockupDeployedArgs {
            version: lockup_contract_config.contract_version,
            account_id: owner_account_id.clone(),
            lockup_update_nonce: lockup_update_nonce.into(),
            lockup_deposit,
        };
        let arguments =
            serde_json::to_vec(&arguments).expect("Failed to serialize lockup init args");

        let promise_id = unsafe {
            sys::promise_then(
                promise_id,
                current_account_id.len() as _,
                current_account_id.as_ptr() as _,
                method_name.len() as _,
                method_name.as_ptr() as _,
                arguments.len() as _,
                arguments.as_ptr() as _,
                0_u128 as *const u128 as _,
                ON_LOCKUP_DEPLOYED.as_gas(),
            )
        };
        unsafe {
            sys::promise_return(promise_id);
        }
    }
}

/// Stores the new lockup contract code internally, doesn't modify the active lockup contract.
/// The input should be the lockup contract code.
/// Returns the contract hash.
/// Requires the caller to attach the deposit to cover the storage cost.
/// Requires the caller to be one of the lockup code deployers.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn prepare_lockup_code() {
    env::setup_panic_hook();
    let contract: Contract = env::state_read().unwrap();
    contract.assert_not_paused();
    let predecessor_id = env::predecessor_account_id();
    require!(
        contract
            .config
            .lockup_code_deployers
            .contains(&predecessor_id),
        "Permission denied"
    );

    const CONTRACT_REGISTER: u64 = 0;
    unsafe {
        sys::input(CONTRACT_REGISTER);
    }
    let (_size, contract_hash) = internal_get_hash_and_size(CONTRACT_REGISTER);
    let starting_storage_usage = env::storage_usage();
    let key = StorageKeys::LockupCode(contract_hash).into_storage_key();
    unsafe {
        sys::storage_write(
            key.len() as _,
            key.as_ptr() as _,
            u64::MAX,
            CONTRACT_REGISTER,
            1,
        );
    }
    let final_storage_usage = env::storage_usage();
    let storage_cost = env::storage_byte_cost()
        .checked_mul((final_storage_usage - starting_storage_usage) as u128)
        .unwrap();
    let attached_deposit = env::attached_deposit();
    require!(
        attached_deposit >= storage_cost,
        "Not enough attached deposit"
    );
    if attached_deposit > storage_cost {
        Promise::new(predecessor_id).transfer(near_sub(attached_deposit, storage_cost));
    }
    let result = serde_json::to_vec(&Base58CryptoHash::from(contract_hash)).unwrap();
    unsafe {
        sys::value_return(result.len() as _, result.as_ptr() as _);
    }
}

fn internal_get_hash_and_size(register_id: u64) -> (u64, CryptoHash) {
    let size = env::register_len(register_id).unwrap();
    let hash_register = register_id + 1;
    unsafe {
        sys::sha256(u64::MAX, register_id, hash_register);
    }
    let hash = env::read_register(hash_register).unwrap();
    (size, hash.try_into().unwrap())
}
