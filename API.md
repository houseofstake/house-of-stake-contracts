# API

The API documentation for the contracts.

## veNEAR

### Structures

```rust
pub struct Config {
    /// The configuration of the current lockup contract code.
    pub lockup_contract_config: Option<LockupContractConfig>,

    /// Initialization arguments for the lockup contract.
    pub unlock_duration_ns: U64,
    /// The account ID of the staking pool whitelist for lockup contract.
    pub staking_pool_whitelist_account_id: AccountId,

    /// The list of account IDs that can store new lockup contract code.
    pub lockup_code_deployers: Vec<AccountId>,

    /// The amount in NEAR required for local storage in veNEAR contract.
    pub local_deposit: NearToken,

    /// The minimum amount in NEAR required for lockup deployment.
    pub min_lockup_deposit: NearToken,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,
}

/// The fixed annual growth rate of veNEAR tokens.
/// Note, the growth rate can be changed in the future through the upgrade mechanism, by introducing
/// timepoints when the growth rate changes.
pub struct VenearGrowthConfigFixedRate {
    /// The growth rate of veNEAR tokens per nanosecond. E.g. 6 / (100 * NUM_SEC_IN_YEAR * 10**9)
    /// means 6% annual growth rate.
    pub annual_growth_rate_ns: Fraction,
}

/// The account details that are stored in the Merkle Tree.
pub struct Account {
    /// The account ID of the account. Required for the security of the Merkle Tree proofs.
    pub account_id: AccountId,
    /// The timestamp in nanoseconds when the account was last updated.
    pub update_timestamp: TimestampNs,
    /// The total NEAR balance of the account as reported by the lockup contract and additional
    /// veNEAR accumulated over time.
    pub balance: VenearBalance,
    /// The total amount of NEAR and veNEAR that was delegated to this account.
    pub delegated_balance: VenearBalance,
    /// The delegation details, in case this account has delegated balance to another account.
    pub delegation: Option<AccountDelegation>,
}

/// Full information about the account
pub struct AccountInfo {
    /// Current account value from the Merkle tree.
    pub account: Account,

    /// Internal account information.
    pub internal: AccountInternal,
}

/// Internal account information from veNEAR contract.
pub struct AccountInternal {
    /// The version of the lockup contract deployed. None means the lockup is not deployed.
    pub lockup_version: Option<Version>,

    /// The amount of NEAR tokens that are retained for the storage of the account.
    pub deposit: NearToken,

    /// The nonce of the last lockup update.
    pub lockup_update_nonce: U64,
}

```

### Methods

```rust

/// Initializes the contract with the given configuration.
#[init]
pub fn new(config: Config, venear_growth_config: VenearGrowthConfigFixedRate);

/// Returns the account info for a given account ID.
pub fn get_account_info(&self, account_id: AccountId) -> Option<AccountInfo>;

/// Returns the number of accounts.
pub fn get_num_accounts(&self) -> u32;

/// Returns the account info for a given index in the Merkle tree.
pub fn get_account_by_index(&self, index: u32) -> Option<AccountInfo>;

/// Returns a list of account info from the given index based on the merkle tree order.
pub fn get_accounts(&self, from_index: Option<u32>, limit: Option<u32>);

/// Returns a list of raw account data from the given index based on the merkle tree order.
pub fn get_accounts_raw(&self, from_index: Option<u32>, limit: Option<u32>);

/// Returns the current contract configuration.
pub fn get_config(&self);

/// Delegate all veNEAR tokens to the given receiver account ID.
/// The receiver account ID must be registered in the contract.
/// Requires 1 yocto NEAR.
#[payable]
pub fn delegate_all(&mut self, receiver_id: AccountId);

/// Undelegate all veNEAR tokens.
/// Requires 1 yocto NEAR.
#[payable]
pub fn undelegate(&mut self);

/// Updates the active lockup contract to the given contract hash and sets the minimum lockup
/// deposit.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_lockup_contract(
    &mut self,
    contract_hash: Base58CryptoHash,
    min_lockup_deposit: NearToken,
);

/// Sets the amount in NEAR required for local storage in veNEAR contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_local_deposit(&mut self, local_deposit: NearToken);

/// Sets the account ID of the staking pool whitelist for lockup contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_staking_pool_whitelist_account_id(
    &mut self,
    staking_pool_whitelist_account_id: AccountId,
);

/// Sets the owner account ID.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_owner_account_id(&mut self, owner_account_id: AccountId);

/// Sets the unlock duration in seconds.
/// Note, this method will only affect new lockups.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_unlock_duration_sec(&mut self, unlock_duration_sec: u32);

/// Sets the list of account IDs that can store new lockup contract code.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_lockup_code_deployers(&mut self, lockup_code_deployers: Vec<AccountId>);

/// Deploys the lockup contract.
/// If the lockup contract is already deployed, the method will fail after the attempt.
/// Requires the caller to attach the deposit for the lockup contract of at least
/// `get_lockup_deployment_cost()`.
/// Requires the caller to already be registered.
#[payable]
pub fn deploy_lockup(&mut self);

/// Called by one of the lockup contracts to update the amount of NEAR locked in the lockup
/// contract.
pub fn on_lockup_update(
    &mut self,
    version: Version,
    owner_account_id: AccountId,
    update: VLockupUpdate,
);

/// Callback after the attempt to deploy the lockup contract.
/// Returns the lockup contract account ID if the deployment was successful.
#[private]
pub fn on_lockup_deployed(
    &mut self,
    version: Version,
    account_id: AccountId,
    lockup_update_nonce: U64,
    lockup_deposit: NearToken,
) -> Option<AccountId>;

/// Returns the account ID for the lockup contract for the given account.
/// Note, the lockup contract is not guaranteed to be deployed.
pub fn get_lockup_account_id(&self, account_id: &AccountId) -> AccountId;

/// Stores the new lockup contract code internally, doesn't modify the active lockup contract.
/// The input should be the lockup contract code.
/// Returns the contract hash.
/// Requires the caller to attach the deposit to cover the storage cost.
/// Requires the caller to be one of the lockup code deployers.
#[payable]
pub fn prepare_lockup_code(&mut self);

/// Returns the current snapshot of the Merkle tree and the global state.
pub fn get_snapshot(&self) -> (MerkleTreeSnapshot, VGlobalState);

/// Returns the proof for the given account and the raw account value.
pub fn get_proof(&self, account_id: AccountId) -> (MerkleProof, VAccount);

/// Registers a new account. If the account is already registered, it refunds the attached
/// deposit.
/// Requires a deposit of at least `storage_balance_bounds().min`.
#[payable]
pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance;

/// Method to match the interface of the storage deposit. Fails with a panic.
#[payable]
pub fn storage_withdraw(&mut self);

/// Returns the minimum required balance to register an account.
pub fn storage_balance_bounds(&self) -> StorageBalanceBounds;

/// Returns the minimum required balance to deploy a lockup.
pub fn get_lockup_deployment_cost(&self) -> NearToken;

/// Returns the storage balance of the given account.
pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance>;

/// Returns the balance of the account in the veNEAR.
pub fn ft_balance_of(&self, account_id: AccountId) -> NearToken;

/// Returns the total supply of the veNEAR.
pub fn ft_total_supply(&self) -> NearToken;

/// Method to match the fungible token interface. Can't be called.
#[payable]
pub fn ft_transfer(&mut self);

/// Method to match the fungible token interface. Can't be called.
#[payable]
pub fn ft_transfer_call(&mut self);

/// Returns the metadata of the veNEAR fungible token.
pub fn ft_metadata(&self) -> serde_json::Value;

/// Private method to migrate the contract state during the contract upgrade.
#[private]
#[init(ignore_state)]
pub fn migrate_state() -> Self;

/// Returns the version of the contract from the Cargo.toml.
pub fn get_version(&self);

/// Upgrades the contract to the new version.
/// Requires the method to be called by the owner.
/// The input is the new contract code.
/// The contract will call `migrate_state` method on the new contract and then return the config,
/// to verify that the migration was successful.
pub fn upgrade();
```
