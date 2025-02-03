use crate::config::LockupContractConfig;
use crate::*;
use common::lockup_update::VLockupUpdate;
use near_sdk::json_types::U64;
use near_sdk::{env, Gas, IntoStorageKey};

const CONTRACT_CODE_EXTRA_STORAGE_BYTES: u64 = 100;

const LOCKUP_DEPLOY_MIN_GAS: Gas = Gas::from_tgas(100);

#[near(serializers=[json])]
pub struct LockupInitArgs {
    version: Version,

    owner_account_id: AccountId,
    // TODO
    lockup_duration: U64,
    staking_pool_whitelist_account_id: AccountId,
}

#[near]
impl Contract {
    /// Called by one of the lockup contracts to update the amount of
    /// NEAR and fungible tokens locked in the lockup contract .
    pub fn on_lockup_update(&mut self, version: Version, update: VLockupUpdate) {
        todo!()
    }
}

/// Internal methods for the contract and lockup.
impl Contract {
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
        self.config.lockup_contract_config = LockupContractConfig {
            contract_size: size,
            contract_version: self.config.lockup_contract_config.contract_version + 1,
            contract_hash,
        }
    }

    pub fn internal_deploy_lockup(&mut self, owner_account_id: AccountId, deposit: NearToken) {
        require!(
            self.config.lockup_contract_config.contract_size > 0,
            "The lockup contract code is not initialized"
        );
        let minimum_deployment_cost = NearToken::from_yoctonear(
            env::storage_byte_cost().as_yoctonear()
                * self.config.lockup_contract_config.contract_size as u128,
        );
        require!(
            deposit >= minimum_deployment_cost,
            "Deposit is not enough to deploy the lockup contract"
        );
        let owner_account_id_hash = hex::encode(&env::sha256(owner_account_id.as_bytes())[0..20]);
        let lockup_account_id: AccountId =
            format!("{}.{}", owner_account_id_hash, env::current_account_id())
                .parse()
                .expect("Failed to create lockup account ID");
        let lockup_account_id = lockup_account_id.as_str();
        let contract_code_key =
            StorageKeys::LockupCode(self.config.lockup_contract_config.contract_hash)
                .into_storage_key();
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
        let arguments = LockupInitArgs {
            version: self.config.lockup_contract_config.contract_version,
            owner_account_id: owner_account_id.clone(),
            lockup_duration: self.config.lockup_duration_ns.clone(),
            staking_pool_whitelist_account_id: self
                .config
                .staking_pool_whitelist_account_id
                .clone(),
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
                &deposit.as_yoctonear() as *const u128 as _,
                LOCKUP_DEPLOY_MIN_GAS.as_gas(),
                1,
            )
        }
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

#[no_mangle]
pub extern "C" fn prepare_lockup_code() {
    env::setup_panic_hook();
    let contract: Contract = env::state_read().unwrap();
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
    let (mut size, contract_hash) = internal_get_hash_and_size(CONTRACT_REGISTER);
    size += CONTRACT_CODE_EXTRA_STORAGE_BYTES;
    let cost = NearToken::from_yoctonear(env::storage_byte_cost().as_yoctonear() * size as u128);
    require!(
        env::attached_deposit() >= cost,
        "Not enough attached deposit"
    );
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
}
