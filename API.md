# API

The API documentation for the contracts.

## Common structures

```rust
/// A rational number serialized as `{numerator, denominator}`. Both fields use
/// `U128` (string-encoded in JSON).
pub struct Fraction {
    pub numerator: U128,
    pub denominator: U128,
}

/// Per-account NEAR + extra veNEAR balance. `near_balance` is the locked NEAR
/// reported by the lockup contract; `extra_venear_balance` is the additional
/// veNEAR accumulated over time at the configured growth rate.
pub struct VenearBalance {
    pub near_balance: NearToken,
    pub extra_venear_balance: NearToken,
}

/// `VenearBalance` variant that pools contributions from many accounts. The
/// `near_balance` part is truncated to milliNEAR on every add to avoid rounding
/// drift in growth calculations; the truncated remainder is folded into
/// `extra_venear_balance` so the total is preserved.
pub struct PooledVenearBalance(VenearBalance);

/// The growth configuration of veNEAR. Currently only `FixedRate` is supported.
pub enum VenearGrowthConfig {
    FixedRate(Box<VenearGrowthConfigFixedRate>),
}

/// The fixed annual growth rate of veNEAR tokens.
/// Note, the growth rate can be changed in the future through the upgrade mechanism, by introducing
/// timepoints when the growth rate changes.
pub struct VenearGrowthConfigFixedRate {
    /// The growth rate of veNEAR tokens per nanosecond. E.g. `6 / (100 * NUM_SEC_IN_YEAR * 10**9)`
    /// means 6% annual growth rate.
    /// Note, the denominator has to be `10**30` to avoid precision issues.
    pub annual_growth_rate_ns: Fraction,
}

/// A single partial delegation entry. `bps` is in basis points (1 = 0.01%).
pub struct DelegationEntry {
    pub account_id: AccountId,
    pub bps: Bps,
}

/// Basis-points newtype around `u16`. Construction validates `value <= 10_000`.
/// Serializes transparently as a plain integer in JSON.
pub struct Bps(u16);

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
    pub delegated_balance: PooledVenearBalance,
    /// The partial delegation entries set by this account (sorted ascending by
    /// `account_id`, sum of `bps` ≤ 10_000). The undelegated remainder
    /// implicitly stays with the owner.
    pub delegations: Vec<DelegationEntry>,
}

/// Borsh-tagged versioning envelope for `Account`. `V1` is the current shape;
/// `V0` is preserved for legacy on-tree records.
pub enum VAccount {
    V0(AccountV0),
    V1(AccountV1),
}

/// The global state of the veNEAR contract and the merkle tree.
pub struct GlobalState {
    pub update_timestamp: TimestampNs,

    pub total_venear_balance: PooledVenearBalance,

    pub venear_growth_config: VenearGrowthConfig,
}

/// Borsh-tagged versioning envelope for `GlobalState`.
pub enum VGlobalState {
    V0(GlobalState),
}

/// The lockup→veNEAR balance update payload. Borsh-tagged to allow future
/// schema additions.
pub enum VLockupUpdate {
    V1(LockupUpdateV1),
}

pub struct LockupUpdateV1 {
    /// The amount of NEAR that is locked in the lockup contract.
    pub locked_near_balance: NearToken,
    /// The timestamp in nanoseconds when the update was created.
    pub timestamp: TimestampNs,
    /// The nonce of the lockup update. Incremented for every new update by the lockup contract.
    pub lockup_update_nonce: U64,
}
```

## veNEAR

### Structures

```rust
/// Identifies the active lockup contract code stored in the veNEAR contract.
pub struct LockupContractConfig {
    pub contract_size: u32,
    pub contract_version: Version,
    pub contract_hash: Base58CryptoHash,
}

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

    /// The list of account IDs that can pause the contract.
    pub guardians: Vec<AccountId>,

    /// Proposed new owner account ID. The account has to accept ownership.
    pub proposed_new_owner_account_id: Option<AccountId>,

    /// Maximum number of partial delegation entries allowed per account.
    pub max_delegations: u32,
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

/// A proof of inclusion in the Merkle tree.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct MerkleProof {
    /// The index of the leaf in the tree.
    pub index: u32,

    /// The corresponding hashes of the siblings in the tree on the path to the root.
    pub path: Vec<Base58CryptoHash>,
}

/// A snapshot of the Merkle tree.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct MerkleTreeSnapshot {
    /// The root hash of the tree.
    pub root: Base58CryptoHash,

    /// The length of the tree.
    pub length: u32,

    /// The block height when the snapshot was taken.
    pub block_height: BlockHeight,
}

#[near(serializers=[json])]
pub struct StorageBalance {
    pub total: NearToken,
    pub available: NearToken,
}

#[near(serializers=[json])]
pub struct StorageBalanceBounds {
    pub min: NearToken,
    pub max: Option<NearToken>,
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
pub fn get_accounts(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AccountInfo>;

/// Returns a list of raw account data from the given index based on the merkle tree order.
pub fn get_accounts_raw(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<&VAccount>;

/// Returns the current contract configuration.
pub fn get_config(&self) -> &Config;

/// Atomically replace the caller's entire delegation set.
/// `entries` must be sorted ascending by account_id (no duplicates), each
/// `bps` in [1, 10_000], total `bps` ≤ 10_000, no self-delegation, every
/// `account_id` registered in veNEAR, and at most `config.max_delegations`
/// entries. Pass an empty Vec to undelegate all. Requires attached deposit
/// ≥ storage growth cost; refunds overpay.
#[payable]
pub fn set_delegations(&mut self, entries: Vec<DelegationEntry>);

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

/// Proposes the new owner account ID.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn propose_new_owner_account_id(&mut self, new_owner_account_id: Option<AccountId>);

/// Accepts the new owner account ID.
/// Can only be called by the new owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn accept_ownership(&mut self);

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

/// Sets the list of account IDs that can pause the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_guardians(&mut self, guardians: Vec<AccountId>);

/// Sets the maximum number of partial delegation entries allowed per account.
/// Existing accounts above the new cap remain valid until they call `set_delegations`.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_max_delegations(&mut self, max_delegations: u32);

/// Checks if the contract is paused.
pub fn is_paused(&self) -> bool;

/// Pauses the contract.
/// Can only be called by the guardian or the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn pause(&mut self);

/// Unpauses the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn unpause(&mut self);

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
pub fn get_version(&self) -> String;

/// Upgrades the contract to the new version.
/// Requires the method to be called by the owner.
/// The input is the new contract code.
/// The contract will call `migrate_state` method on the new contract and then return the config,
/// to verify that the migration was successful.
pub fn upgrade();
```

## Lockup

### Structures

```rust
/// Persistent state of a single lockup contract instance. One lockup contract
/// is deployed per user by the veNEAR contract.
pub struct LockupContract {
    /// The account ID of the owner.
    pub owner_account_id: AccountId,
    /// Account ID of the veNEAR contract that deployed this lockup.
    pub venear_account_id: AccountId,
    /// Account ID of the staking pool whitelist contract.
    pub staking_pool_whitelist_account_id: AccountId,
    /// Information about the currently selected staking pool. `None` means no
    /// pool is selected.
    pub staking_information: Option<StakingInformation>,
    /// The unlock duration in nanoseconds.
    pub unlock_duration_ns: u64,
    /// The amount of NEAR currently locked (in yoctoNEAR).
    pub venear_locked_balance: Balance,
    /// The timestamp (ns) at which `venear_pending_balance` becomes withdrawable.
    pub venear_unlock_timestamp: Timestamp,
    /// The amount of NEAR scheduled to unlock at `venear_unlock_timestamp`.
    pub venear_pending_balance: Balance,
    /// The nonce of the next lockup→veNEAR update. Monotonically increasing.
    pub lockup_update_nonce: u64,
    /// The version of this lockup contract code, tracked by the veNEAR contract.
    pub version: Version,
    /// The minimum NEAR balance required for lockup deployment.
    pub min_lockup_deposit: NearToken,
}

/// Status of in-flight transactions to the staking pool contract.
pub enum TransactionStatus {
    /// There are no transactions in progress.
    Idle,
    /// There is a transaction in progress.
    Busy,
}

/// Information about the currently selected staking pool.
pub struct StakingInformation {
    /// The account ID of the staking pool contract.
    pub staking_pool_account_id: AccountId,
    /// Whether a transaction with the staking pool is currently in progress.
    pub status: TransactionStatus,
    /// The amount of tokens deposited from this lockup to the staking pool.
    /// Note: the unstaked balance on the staking pool may be higher due to rewards.
    pub deposit_amount: NearToken,
}
```

### Methods

```rust
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
) -> Self;

/// Returns the account ID of the owner.
pub fn get_owner_account_id(&self) -> AccountId;

/// Returns the account ID of the selected staking pool.
pub fn get_staking_pool_account_id(&self) -> Option<AccountId>;

/// Returns the amount of tokens that were deposited to the staking pool.
/// NOTE: The actual balance can be larger than this known deposit balance due to staking
/// rewards acquired on the staking pool.
/// To refresh the amount the owner can call `refresh_staking_pool_balance`.
pub fn get_known_deposited_balance(&self) -> NearToken;

/// Returns the balance of the account owner.
/// Note: This is the same as `get_balance`.
pub fn get_owners_balance(&self) -> NearToken;

/// Returns total balance of the account including tokens deposited to the staking pool.
pub fn get_balance(&self) -> NearToken;

/// Returns the amount of tokens the owner can transfer from the account.
pub fn get_liquid_owners_balance(&self) -> NearToken;

/// Returns the version of the Lockup contract.
pub fn get_version(&self) -> Version;

/// OWNER'S METHOD
///
/// Requires 75 TGas (3 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Selects staking pool contract at the given account ID. The staking pool first has to be
/// checked against the staking pool whitelist contract.
#[payable]
pub fn select_staking_pool(&mut self, staking_pool_account_id: AccountId) -> Promise;

/// OWNER'S METHOD
///
/// Requires 25 TGas (1 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Unselects the current staking pool.
/// It requires that there are no known deposits left on the currently selected staking pool.
#[payable]
pub fn unselect_staking_pool(&mut self);

/// OWNER'S METHOD
///
/// Requires 100 TGas (4 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Deposits the given extra amount to the staking pool
#[payable]
pub fn deposit_to_staking_pool(&mut self, amount: NearToken) -> Promise;

/// OWNER'S METHOD
///
/// Requires 125 TGas (5 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Deposits and stakes the given extra amount to the selected staking pool
#[payable]
pub fn deposit_and_stake(&mut self, amount: NearToken) -> Promise;

/// OWNER'S METHOD
///
/// Requires 75 TGas (3 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Retrieves total balance from the staking pool and remembers it internally.
/// This method is helpful when the owner received some rewards for staking and wants to
/// transfer them back to this account for withdrawal. In order to know the actual liquid
/// balance on the account, this contract needs to query the staking pool.
#[payable]
pub fn refresh_staking_pool_balance(&mut self) -> Promise;

/// OWNER'S METHOD
///
/// Requires 125 TGas (5 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Withdraws the given amount from the staking pool
#[payable]
pub fn withdraw_from_staking_pool(&mut self, amount: NearToken) -> Promise;

/// OWNER'S METHOD
///
/// Requires 175 TGas (7 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Tries to withdraws all unstaked balance from the staking pool
#[payable]
pub fn withdraw_all_from_staking_pool(&mut self) -> Promise;

/// OWNER'S METHOD
///
/// Requires 125 TGas (5 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Stakes the given extra amount at the staking pool
#[payable]
pub fn stake(&mut self, amount: NearToken) -> Promise;

/// OWNER'S METHOD
///
/// Requires 125 TGas (5 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Unstakes the given amount at the staking pool
#[payable]
pub fn unstake(&mut self, amount: NearToken) -> Promise;

/// OWNER'S METHOD
///
/// Requires 125 TGas (5 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Unstakes all tokens from the staking pool
#[payable]
pub fn unstake_all(&mut self) -> Promise;

/// OWNER'S METHOD
///
/// Requires 50 TGas (2 * BASE_GAS)
/// Requires 1 yoctoNEAR attached
///
/// Transfers the given amount to the given receiver account ID.
#[payable]
pub fn transfer(&mut self, amount: NearToken, receiver_id: AccountId) -> Promise;

/// OWNER'S METHOD
///
/// Requires 1 yoctoNEAR attached
/// Requires no locked balances or staking pool deposits.
///
/// Removes the lockup contract and transfers all NEAR to the initial owner.
#[payable]
pub fn delete_lockup(&mut self) -> Promise;

/// Called after a given `staking_pool_account_id` was checked in the whitelist.
#[private]
pub fn on_whitelist_is_whitelisted(
    &mut self,
    #[callback] is_whitelisted: bool,
    staking_pool_account_id: AccountId,
) -> bool;

/// Called after a deposit amount was transferred out of this account to the staking pool.
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_deposit(&mut self, amount: NearToken) -> bool;

/// Called after a deposit amount was transferred out of this account to the staking pool and it
/// was staked on the staking pool.
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_deposit_and_stake(&mut self, amount: NearToken) -> bool;

/// Called after the given amount was requested to transfer out from the staking pool to this
/// account.
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_withdraw(&mut self, amount: NearToken) -> bool;

/// Called after the extra amount stake was staked in the staking pool contract.
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_stake(&mut self, amount: NearToken) -> bool;

/// Called after the given amount was unstaked at the staking pool contract.
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_unstake(&mut self, amount: NearToken) -> bool;

/// Called after all tokens were unstaked at the staking pool contract
/// This method needs to update staking pool status.
#[private]
pub fn on_staking_pool_unstake_all(&mut self) -> bool;

/// Called after the request to get the current total balance from the staking pool.
#[private]
pub fn on_get_account_total_balance(&mut self, #[callback] total_balance: NearToken);

/// Called after the request to get the current unstaked balance to withdraw everything by th
/// owner.
#[private]
pub fn on_get_account_unstaked_balance_to_withdraw_by_owner(
    &mut self,
    #[callback] unstaked_balance: NearToken,
) -> PromiseOrValue<bool>;

/// Returns the amount of NEAR locked in the lockup contract
pub fn get_venear_locked_balance(&self) -> NearToken;

/// Returns the timestamp in nanoseconds when the pending amount will be unlocked
pub fn get_venear_unlock_timestamp(&self) -> TimestampNs;

/// Returns the nonce of the lockup update
pub fn get_lockup_update_nonce(&self) -> U64;

/// Returns the amount of NEAR that is pending to be unlocked
pub fn get_venear_pending_balance(&self) -> NearToken;

/// Returns the amount of NEAR that is liquid (the NEAR that can be locked)
pub fn get_venear_liquid_balance(&self) -> NearToken;

/// OWNER'S METHOD
///
/// Requires 1 yoctoNEAR attached
///
/// Locks the NEAR in the lockup contract.
/// You can specify the amount of NEAR to lock, or if you don't specify it, all the liquid NEAR
/// will be locked.
#[payable]
pub fn lock_near(&mut self, amount: Option<NearToken>);

/// OWNER'S METHOD
///
/// Requires 1 yoctoNEAR attached
///
/// Starts the unlocking process of the locked NEAR in the lockup contract.
/// You specify the amount of near to unlock, or if you don't specify it, all the locked NEAR
/// will be unlocked.
/// (works similarly to unstaking from a staking pool).
#[payable]
pub fn begin_unlock_near(&mut self, amount: Option<NearToken>);

/// OWNER'S METHOD
///
/// Requires 1 yoctoNEAR attached
/// Requires that the unlock timestamp is reached
///
/// Finishes the unlocking process of the NEAR in the lockup contract.
/// You can specify the amount of NEAR to unlock, or if you don't specify it, all the pending
/// NEAR will be unlocked.
#[payable]
pub fn end_unlock_near(&mut self, amount: Option<NearToken>);

/// OWNER'S METHOD
///
/// Requires 1 yoctoNEAR attached
///
/// Locks the pending NEAR in the lockup contract.
/// You can specify the amount of NEAR to lock, or if you don't specify it, all the pending NEAR
/// will be locked.
#[payable]
pub fn lock_pending_near(&mut self, amount: Option<NearToken>);
```

## Voting

The voting contract supports two proposal flows that share a single `Config`,
`Proposal`, and `ProposalStatus`. Each proposal selects its flow (`Classic` or
`FastTrack`) at creation time via `ProposalFlow`. Doc comments below mark
fields/variants that only apply to one of the flows.

### Structures

```rust
/// Configuration of the voting contract. Governs both Classic and FastTrack flows.
pub struct Config {
    /// The account ID of the veNEAR contract.
    pub venear_account_id: AccountId,

    /// Account IDs that can approve / reject / slash proposals.
    pub reviewer_ids: Vec<AccountId>,

    /// Council member account IDs (can veto proposals).
    pub council_ids: Vec<AccountId>,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,

    /// Voting period for Classic proposals (nanoseconds).
    pub classic_voting_duration_ns: U64,

    /// Voting period for FastTrack proposals (nanoseconds).
    pub fast_track_voting_duration_ns: U64,

    /// Timelock duration after voting ends (Classic flow only).
    pub timelock_duration_ns: U64,

    /// Base proposal fee in addition to the storage cost.
    pub base_proposal_fee: NearToken,

    /// Bond required to create a FastTrack proposal. Forwarded to the treasury
    /// when the proposal is approved or slashed; refundable while in Created/
    /// Rejected/Expired status via `claim_bond`.
    pub bond_amount: NearToken,

    /// Treasury account that receives forfeited FastTrack bonds.
    pub treasury_account_id: AccountId,

    /// Storage fee required to store a vote for an active proposal.
    pub vote_storage_fee: NearToken,

    /// The list of account IDs that can pause the contract.
    pub guardians: Vec<AccountId>,

    /// Max time a Classic proposal may stay in `Created` before expiring (0 = no expiration).
    pub classic_proposal_expiration_ns: U64,

    /// Max time a FastTrack proposal may stay in `Created` before expiring (0 = no expiration).
    pub fast_track_proposal_expiration_ns: U64,

    /// Proposed new owner; must accept ownership.
    pub proposed_new_owner_account_id: Option<AccountId>,

    /// Quorum threshold in basis points (e.g. 3500 = 35% of total supply).
    pub quorum_threshold_bps: Bps,

    /// Absolute minimum veNEAR required for quorum.
    pub quorum_floor: NearToken,

    /// TODO: remove with the next state migration. Unused since approval requires an
    /// explicit majority type; retained only to keep the stored layout unchanged.
    pub approval_threshold_bps: Bps,

    /// Simple-majority threshold in basis points.
    pub simple_majority_threshold_bps: Bps,

    /// Strong-majority threshold in basis points.
    pub strong_majority_threshold_bps: Bps,

    /// Sandbox pre-voting duration (FastTrack flow).
    pub sandbox_duration_ns: U64,

    /// "For" votes threshold to graduate from Sandbox to Scheduled (basis points).
    pub sandbox_threshold_bps: Bps,

    /// Maximum number of proposals simultaneously in Sandbox/Scheduled/Voting/Timelock.
    /// Approved proposals beyond this cap park in the pending queue.
    pub max_active_proposals: u32,
}

/// Metadata for a proposal.
pub struct ProposalMetadata {
    /// The title of the proposal.
    pub title: Option<String>,

    /// The description of the proposal.
    pub description: Option<String>,

    /// The link to the proposal.
    pub link: Option<String>,
}

/// Which lifecycle a proposal follows. Selected at creation time.
pub enum ProposalFlow {
    Classic,
    FastTrack,
}

/// The fixed voting options for proposals.
pub enum VoteOption {
    For,
    Against,
    Abstain,
}

/// Majority type selected by the reviewer at approval time. Determines which
/// configured threshold (`simple_majority_threshold_bps` vs
/// `strong_majority_threshold_bps`) is recorded on the proposal as
/// `approval_threshold_bps`.
pub enum MajorityType {
    Simple,
    Strong,
}

/// A single action that the voting contract can execute on behalf of a passed proposal.
pub enum ProposalAction {
    /// Execute a function call on a target contract.
    FunctionCall {
        receiver_id: AccountId,
        method_name: String,
        args: Base64VecU8,
        deposit: NearToken,
        gas: Gas,
    },
    /// Transfer NEAR to a target account.
    Transfer {
        receiver_id: AccountId,
        amount: NearToken,
    },
}

/// Unified proposal record. Most fields apply to both flows; `timelock_duration_ns`
/// is Classic-only, and `sandbox_*` / `bond_amount` are FastTrack-only.
pub struct Proposal {
    /// The unique identifier of the proposal, generated automatically.
    pub id: ProposalId,
    /// The timestamp in nanoseconds when the proposal was created, generated automatically.
    pub creation_time_ns: U64,
    /// The account ID of the proposer.
    pub proposer_id: AccountId,
    /// The account ID of the reviewer who approved the proposal.
    pub reviewer_id: Option<AccountId>,
    /// The account ID of the council member who vetoed the proposal (if any).
    pub rejecter_id: Option<AccountId>,
    /// The timestamp when a reviewer approved the proposal.
    pub approval_time_ns: Option<U64>,
    /// The timestamp when the proposal enters the Voting status.
    pub voting_start_time_ns: Option<U64>,
    /// The voting duration in nanoseconds, recorded per-proposal from config at creation time.
    pub voting_duration_ns: U64,
    /// The deadline in nanoseconds by which the proposal must be approved. 0 means no expiration.
    pub expiration_ns: U64,
    /// The snapshot of the contract state and global state. Fetched when the proposal is approved.
    pub snapshot_and_state: Option<SnapshotAndState>,
    /// Aggregated votes per voting option (one entry per `VoteOption` index).
    pub votes: Vec<VoteStats>,
    /// The total aggregated voting information across all voting options.
    pub total_votes: VoteStats,
    /// The status of the proposal.
    pub status: ProposalStatus,
    /// Quorum threshold in basis points.
    pub quorum_threshold_bps: Bps,
    /// Absolute minimum veNEAR required for quorum.
    pub quorum_floor: NearToken,
    /// Approval threshold in basis points, set at approval time from the reviewer's
    /// `MajorityType`.
    pub approval_threshold_bps: Bps,
    /// Optional list of on-chain actions to execute when the proposal succeeds.
    pub actions: Option<Vec<ProposalAction>>,
    /// Which flow this proposal follows.
    pub flow: ProposalFlow,
    /// Classic only. Timelock duration in nanoseconds, recorded per-proposal from config.
    pub timelock_duration_ns: U64,
    /// FastTrack only. The timestamp when the proposal entered Sandbox.
    pub sandbox_start_time_ns: Option<U64>,
    /// FastTrack only. Bond locked on the proposal until approval/slash/refund.
    pub bond_amount: NearToken,
    /// FastTrack only. Sandbox pre-voting duration in nanoseconds.
    pub sandbox_duration_ns: U64,
    /// FastTrack only. Sandbox graduation threshold in basis points.
    pub sandbox_threshold_bps: Bps,
}

/// The proposal information structure that contains the proposal and its metadata.
pub struct ProposalInfo {
    #[serde(flatten)]
    pub proposal: Proposal,
    #[serde(flatten)]
    pub metadata: ProposalMetadata,
}

/// Lifecycle status of a proposal. The same enum is used for both flows;
/// each flow only reaches a subset of these variants.
pub enum ProposalStatus {
    /// Created and waiting for a reviewer.
    Created,
    /// Reviewer rejected the proposal before approval.
    Rejected,
    /// Legacy: pre-merge "Approval" state from older state.
    ApprovalLegacy,
    /// Voting is in progress.
    Voting,
    /// Legacy: pre-merge "Finished" state from older state.
    FinishLegacy,
    /// Council member vetoed the proposal.
    /// Classic: only valid during Timelock.
    /// FastTrack: valid while Scheduled or Voting.
    Vetoed,
    /// Classic only. Voting ended successfully; awaiting potential council veto
    /// before `Succeeded` / `Executable`.
    Timelock,
    /// Expired before being approved by a reviewer.
    Expired,
    /// Voting succeeded (quorum met, approval threshold met) and either had no
    /// actions or its actions executed successfully.
    Succeeded,
    /// Voting concluded but quorum or approval threshold was not met.
    Defeated,
    /// Voting succeeded with on-chain actions ready for execution.
    Executable,
    /// On-chain actions dispatched, awaiting callback.
    InProgress,
    /// On-chain action execution failed.
    Failed,
    /// FastTrack only. Reviewer slashed the proposal; bond forwarded to treasury.
    Slashed,
    /// FastTrack only. Pre-voting period during which only "For" votes are accepted.
    Sandbox,
    /// FastTrack only. Graduated from Sandbox; queued to start voting on the next Monday.
    Scheduled,
    /// Approved but waiting for an active slot to free up before activation.
    Queued,
}

/// The snapshot of the Merkle tree and the global state at the moment when the proposal was
/// approved.
pub struct SnapshotAndState {
    /// The snapshot of the Merkle tree at the moment when the proposal was approved.
    pub snapshot: MerkleTreeSnapshot,
    /// The timestamp in nanoseconds when the global state was last updated.
    pub timestamp_ns: TimestampNs,
    /// The total amount of veNEAR tokens at the moment when the proposal was approved.
    pub total_venear: NearToken,
    /// The growth configuration of the veNEAR tokens from the global state.
    pub venear_growth_config: VenearGrowthConfig,
}

/// The vote statistics structure that contains the total amount of veNEAR tokens and the total
/// number of votes.
pub struct VoteStats {
    /// The total venear balance at the updated timestamp.
    pub total_venear: NearToken,

    /// The total number of votes.
    pub total_votes: u32,
}

/// Optional vote argument bundled with `take_snapshot_and_vote`.
pub struct VotePayload {
    /// The chosen voting option.
    pub vote: VoteOption,
    /// Merkle proof of the voter's account in the proposal's snapshot.
    pub merkle_proof: MerkleProof,
    /// The voter's account state from the snapshot.
    pub v_account: VAccount,
}

/// Snapshot of the proposal scheduler's currently-active proposals and pending FIFO queue.
pub struct QueueState {
    /// Proposal IDs currently occupying an active slot.
    pub active_proposals: Vec<ProposalId>,
    /// Proposal IDs waiting in FIFO order to be promoted into active slots.
    pub pending_queue: Vec<ProposalId>,
}
```

### Methods

```rust
/// Initializes the contract with the given configuration.
#[init]
pub fn new(config: Config) -> Self;

/// Returns the current contract configuration.
pub fn get_config(&self) -> &Config;

/// Updates the account ID of the veNEAR contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_venear_account_id(&mut self, venear_account_id: AccountId);

/// Updates the list of account IDs that can review proposals.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_reviewer_ids(&mut self, reviewer_ids: Vec<AccountId>);

/// Updates the list of council member account IDs who can veto proposals.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_council_ids(&mut self, council_ids: Vec<AccountId>);

/// Updates the Classic-flow voting duration in seconds.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_classic_voting_duration(&mut self, voting_duration_sec: u32);

/// Updates the FastTrack-flow voting duration in seconds.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_fast_track_voting_duration(&mut self, voting_duration_sec: u32);

/// Updates the timelock duration in seconds (Classic flow).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_timelock_duration(&mut self, timelock_duration_sec: u32);

/// Updates the base fee required to create a proposal.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_base_proposal_fee(&mut self, base_proposal_fee: NearToken);

/// Updates the FastTrack bond amount.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_bond_amount(&mut self, bond_amount: NearToken);

/// Updates the treasury account ID that receives forfeited FastTrack bonds.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_treasury_account_id(&mut self, treasury_account_id: AccountId);

/// Updates the Classic proposal expiration duration in seconds. Set to 0 to disable.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_classic_proposal_expiration(&mut self, proposal_expiration_sec: u32);

/// Updates the FastTrack proposal expiration duration in seconds. Set to 0 to disable.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_fast_track_proposal_expiration(&mut self, proposal_expiration_sec: u32);

/// Updates the quorum threshold in basis points (e.g. 3500 = 35%).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_quorum_threshold_bps(&mut self, quorum_threshold_bps: Bps);

/// Updates the quorum floor (absolute minimum veNEAR required for quorum).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_quorum_floor(&mut self, quorum_floor: NearToken);

/// TODO: remove with the next state migration, along with `approval_threshold_bps`.
/// The value no longer affects approvals — use the simple/strong majority setters instead.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_approval_threshold_bps(&mut self, approval_threshold_bps: Bps);

/// Updates the simple-majority threshold in basis points (e.g. 5000 = 50%).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_simple_majority_threshold_bps(&mut self, simple_majority_threshold_bps: Bps);

/// Updates the strong-majority threshold in basis points (e.g. 6667 ≈ 66.67%).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_strong_majority_threshold_bps(&mut self, strong_majority_threshold_bps: Bps);

/// Updates the FastTrack sandbox duration in seconds.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_sandbox_duration(&mut self, sandbox_duration_sec: u32);

/// Updates the FastTrack sandbox graduation threshold in basis points (e.g. 3000 = 30%).
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_sandbox_threshold_bps(&mut self, sandbox_threshold_bps: Bps);

/// Updates the maximum number of simultaneously-active proposals
/// (Sandbox/Scheduled/Voting/Timelock). Approved proposals beyond the cap
/// park in the pending queue.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_max_active_proposals(&mut self, max_active_proposals: u32);

/// Sets the list of account IDs that can pause the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_guardians(&mut self, guardians: Vec<AccountId>);

/// Proposes the new owner account ID.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn propose_new_owner_account_id(&mut self, new_owner_account_id: Option<AccountId>);

/// Accepts the new owner account ID.
/// Can only be called by the proposed new owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn accept_ownership(&mut self);

/// Checks if the contract is paused.
pub fn is_paused(&self) -> bool;

/// Pauses the contract.
/// Can only be called by a guardian or the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn pause(&mut self);

/// Unpauses the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn unpause(&mut self);

/// Creates a new proposal in the selected `flow` with the given metadata and
/// optional on-chain actions. Required deposit covers the storage cost,
/// `base_proposal_fee`, and (FastTrack only) `bond_amount`.
/// If actions are provided, the proposal lands in `Executable` after voting
/// succeeds (and timelock, for Classic) instead of `Succeeded`; anyone can
/// call `execute_proposal` to trigger them.
#[payable]
pub fn create_proposal(
    &mut self,
    metadata: ProposalMetadata,
    actions: Option<Vec<ProposalAction>>,
    flow: ProposalFlow,
) -> ProposalId;

/// Returns the proposal information by the given proposal ID.
pub fn get_proposal(&self, proposal_id: ProposalId) -> Option<ProposalInfo>;

/// Returns the number of proposals.
pub fn get_num_proposals(&self) -> u32;

/// Returns a list of proposals from the given index based on the proposal ID order.
pub fn get_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo>;

/// Approves a proposal. `majority_type` is required for both flows and selects
/// which configured majority threshold is recorded on the proposal.
/// If an active slot is available the proposal is activated immediately
/// (Classic → Voting, FastTrack → Sandbox) and a snapshot fetch is scheduled;
/// otherwise the proposal is queued (`ProposalStatus::Queued`) and the
/// `PromiseOrValue` resolves to the queued `ProposalInfo`.
/// Requires 1 yocto attached to the call.
/// Can only be called by the reviewers.
#[payable]
pub fn approve_proposal(
    &mut self,
    proposal_id: ProposalId,
    majority_type: MajorityType,
) -> PromiseOrValue<Option<ProposalInfo>>;

/// Rejects a proposal that is still in the `Created` status. The bond (if any)
/// becomes claimable via `claim_bond`.
/// Requires 1 yocto attached to the call.
/// Can only be called by the reviewers.
#[payable]
pub fn reject_proposal(&mut self, proposal_id: ProposalId);

/// Vetoes a proposal.
/// Classic: only valid during `Timelock`.
/// FastTrack: valid during `Voting` or `Scheduled`.
/// Requires 1 yocto attached to the call.
/// Can only be called by the council members.
#[payable]
pub fn veto_proposal(&mut self, proposal_id: ProposalId);

/// Waives the veto right during the Classic timelock period, ending the
/// timelock immediately so the proposal advances to `Executable` / `Succeeded`.
/// Requires 1 yocto attached to the call.
/// Can only be called by the council members.
#[payable]
pub fn noveto_proposal(&mut self, proposal_id: ProposalId);

/// Slashes a FastTrack proposal that is still in `Created`. The bond is
/// forwarded to `treasury_account_id` and is not refundable.
/// Requires 1 yocto attached to the call.
/// Can only be called by the reviewers.
#[payable]
pub fn slash_proposal(&mut self, proposal_id: ProposalId) -> PromiseOrValue<()>;

/// Refunds the FastTrack bond to the proposer. Only valid while the proposal
/// is in `Expired` or `Rejected`; in any other terminal state the bond has
/// already been forwarded to the treasury.
pub fn claim_bond(&mut self, proposal_id: ProposalId) -> Promise;

/// Executes the on-chain actions for a proposal that has passed voting (and
/// timelock for Classic). Can be called by anyone. The proposal must be in
/// `Executable` status. Actions are executed sequentially. Status moves to
/// `InProgress` during execution, then to `Succeeded` or `Failed` based on
/// the callback result.
pub fn execute_proposal(&mut self, proposal_id: ProposalId) -> Promise;

/// A callback after the snapshot is received for approving the proposal.
#[private]
pub fn on_get_snapshot(
    &mut self,
    #[callback] snapshot_and_state: (MerkleTreeSnapshot, VGlobalState),
    proposal_id: ProposalId,
) -> ProposalInfo;

/// A callback after the proposal actions have been executed.
/// Sets the proposal status to `Succeeded` if all actions succeeded, or `Failed` otherwise.
#[private]
pub fn on_execute_proposal(&mut self, proposal_id: ProposalId);

/// Cast a vote for the given proposal and the given voting option.
/// The caller has to provide a merkle proof and the account state from the snapshot.
/// The caller should match the account ID in the account state.
/// During FastTrack `Sandbox`, only `For` votes are accepted.
/// Requires a deposit to cover the storage fee or at least 1 yoctoNEAR if changing the vote.
#[payable]
pub fn vote(
    &mut self,
    proposal_id: ProposalId,
    vote: VoteOption,
    merkle_proof: MerkleProof,
    v_account: VAccount,
);

/// Fetches a fresh veNEAR snapshot for a proposal already in `Sandbox` or
/// `Voting` that does not yet have one, optionally chaining a vote in the
/// same transaction.
#[payable]
pub fn take_snapshot_and_vote(
    &mut self,
    proposal_id: ProposalId,
    vote: Option<VotePayload>,
) -> Promise;

/// Returns the vote of the given account ID and proposal ID.
pub fn get_vote(&self, account_id: AccountId, proposal_id: ProposalId) -> Option<u8>;

/// Promotes proposals from the pending queue into freed active slots.
/// Can be called by anyone. Most state-mutating reviewer and voting calls
/// already advance the queue internally; this method is available for
/// callers who want to nudge it explicitly.
pub fn advance_queue(&mut self);

/// Returns the current active proposals and the FIFO pending queue.
pub fn get_queue_state(&self) -> QueueState;

/// Private method to migrate the contract state during the contract upgrade.
#[private]
#[init(ignore_state)]
pub fn migrate_state() -> Self;

/// Returns the version of the contract from the Cargo.toml.
pub fn get_version(&self) -> String;

/// Upgrades the contract to the new version.
/// Requires the method to be called by the owner.
/// The input is the new contract code.
/// The contract will call `migrate_state` method on the new contract and then return the config,
/// to verify that the migration was successful.
pub fn upgrade();
```
