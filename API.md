# API

The API documentation for the contracts.

## Common structures

```rust
/// The fixed annual growth rate of veNEAR tokens.
/// Note, the growth rate can be changed in the future through the upgrade mechanism, by introducing
/// timepoints when the growth rate changes.
pub struct VenearGrowthConfigFixedRate {
    /// The growth rate of veNEAR tokens per nanosecond. E.g. `6 / (100 * NUM_SEC_IN_YEAR * 10**9)`
    /// means 6% annual growth rate.
    /// Note, the denominator has to be `10**30` to avoid precision issues.
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

/// The global state of the veNEAR contract and the merkle tree.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct GlobalState {
    pub update_timestamp: TimestampNs,

    pub total_venear_balance: VenearBalance,

    pub venear_growth_config: VenearGrowthConfig,
}
```

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

    /// The list of account IDs that can pause the contract.
    pub guardians: Vec<AccountId>,

    /// Proposed new owner account ID. The account has to accept ownership.
    pub proposed_new_owner_account_id: Option<AccountId>,
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

/// Checks if the contract is paused.
pub fn is_paused(&self) -> bool;

/// Pauses the contract.
/// Can only be called by the guardian.
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
pub fn get_version(&self);

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
pub fn get_version(&self);

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

### Structures

```rust
/// The configuration of the voting contract.
#[derive(Debug, Clone)]
#[near(serializers=[borsh, json])]
pub struct Config {
    /// The account ID of the veNEAR contract.
    pub venear_account_id: AccountId,

    /// The account ID that can approve or reject proposals.
    pub reviewer_ids: Vec<AccountId>,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,

    /// The maximum duration of the voting period in nanoseconds.
    pub voting_duration_ns: U64,

    /// The maximum number of voting options per proposal.
    pub max_number_of_voting_options: u8,

    /// The base fee in addition to the storage fee required to create a proposal.
    pub base_proposal_fee: NearToken,

    /// Storage fee required to store a vote for an active proposal. It can be refunded once the
    /// proposal is finalized.
    pub vote_storage_fee: NearToken,

    /// The list of account IDs that can pause the contract.
    pub guardians: Vec<AccountId>,

    /// Proposed new owner account ID. The account has to accept ownership.
    pub proposed_new_owner_account_id: Option<AccountId>,
}

/// Metadata for a proposal.
pub struct ProposalMetadata {
    /// The title of the proposal.
    pub title: Option<String>,

    /// The description of the proposal.
    pub description: Option<String>,

    /// The link to the proposal.
    pub link: Option<String>,

    /// The voting options for the proposal.
    pub voting_options: Vec<String>,
}

/// The proposal structure that contains all the information about a proposal.
pub struct Proposal {
    /// The unique identifier of the proposal, generated automatically.
    pub id: ProposalId,
    /// The timestamp in nanoseconds when the proposal was created, generated automatically.
    pub creation_time_ns: U64,
    /// The account ID of the proposer.
    pub proposer_id: AccountId,
    /// The account ID of the reviewer, who approved or rejected the proposal.
    pub reviewer_id: Option<AccountId>,
    /// The timestamp when the voting starts, provided by the reviewer.
    pub voting_start_time_ns: Option<U64>,
    /// The voting duration in nanoseconds, generated from the config.
    pub voting_duration_ns: U64,
    /// The flag indicating if the proposal was rejected by the reviewer.
    pub rejected: bool,
    /// The snapshot of the contract state and global state. Fetched when the proposal is approved.
    pub snapshot_and_state: Option<SnapshotAndState>,
    /// Aggregated votes per voting option.
    pub votes: Vec<VoteStats>,
    /// The total aggregated voting information across all voting options.
    pub total_votes: VoteStats,
    /// The status of the proposal. It's optional and can be computed from the proposal itself.
    pub status: ProposalStatus,
}

/// The proposal information structure that contains the proposal and its metadata.
pub struct ProposalInfo {
    #[serde(flatten)]
    pub proposal: Proposal,
    #[serde(flatten)]
    pub metadata: ProposalMetadata,
}

/// The status of the proposal
pub enum ProposalStatus {
    /// The proposal was created and is waiting for the approver to approve or reject it.
    Created,
    /// The proposal was rejected by the approver.
    Rejected,
    /// The proposal was approved by the approver and is waiting for the voting to start.
    Approved,
    /// The proposal is in the voting phase.
    Voting,
    /// The proposal voting is finished and the results are available.
    Finished,
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

/// Updates the maximum duration of the voting period in seconds.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_voting_duration(&mut self, voting_duration_sec: u32);

/// Updates the base fee required to create a proposal.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_base_proposal_fee(&mut self, base_proposal_fee: NearToken);

/// Updates the storage fee required to store a vote for an active proposal.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
/// Will panic if called, because it requires contract upgrade to change the storage fee.
#[payable]
pub fn set_vote_storage_fee(&mut self, _vote_storage_fee: NearToken);

/// Updates the maximum number of voting options per proposal.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_max_number_of_voting_options(&mut self, max_number_of_voting_options: u8);

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

/// Sets the list of account IDs that can pause the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn set_guardians(&mut self, guardians: Vec<AccountId>);

/// Checks if the contract is paused.
pub fn is_paused(&self) -> bool;

/// Pauses the contract.
/// Can only be called by the guardian.
/// Requires 1 yocto NEAR.
#[payable]
pub fn pause(&mut self);

/// Unpauses the contract.
/// Can only be called by the owner.
/// Requires 1 yocto NEAR.
#[payable]
pub fn unpause(&mut self);

/// Creates a new proposal with the given metadata.
/// The proposal is created by the predecessor account and requires a deposit to cover the
/// storage and the base proposal fee.
#[payable]
pub fn create_proposal(&mut self, metadata: ProposalMetadata) -> ProposalId;

/// Returns the proposal information by the given proposal ID.
pub fn get_proposal(&self, proposal_id: ProposalId) -> Option<ProposalInfo>;

/// Returns the number of proposals.
pub fn get_num_proposals(&self) -> u32;

/// Returns a list of proposals from the given index based on the proposal ID order.
pub fn get_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo>;

/// Returns the number of approved proposals.
pub fn get_num_approved_proposals(&self) -> u32;

/// Returns a list of approved proposals from the given index based on the approved proposals
/// order.
pub fn get_approved_proposals(&self, from_index: u32, limit: Option<u32>) -> Vec<ProposalInfo>;

/// Approves the proposal to start the voting process.
/// An optional voting start time in seconds can be provided to delay the start of the voting.
/// Requires 1 yocto attached to the call.
/// Can only be called by the reviewers.
#[payable]
pub fn approve_proposal(
    &mut self,
    proposal_id: ProposalId,
    voting_start_time_sec: Option<u32>,
) -> Promise;

/// Rejects the proposal.
/// Requires 1 yocto attached to the call.
/// Can only be called by the reviewers.
#[payable]
pub fn reject_proposal(&mut self, proposal_id: ProposalId);

/// A callback after the snapshot is received for approving the proposal.
#[private]
pub fn on_get_snapshot(
    &mut self,
    #[callback] snapshot_and_state: (MerkleTreeSnapshot, VGlobalState),
    reviewer_id: AccountId,
    proposal_id: ProposalId,
    voting_start_time_sec: Option<u32>,
) -> ProposalInfo;

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

/// Cast a vote for the given proposal and the given voting option.
/// The caller has to provide a merkle proof and the account state from the snapshot.
/// The caller should match the account ID in the account state.
/// Requires a deposit to cover the storage fee or at least 1 yoctoNEAR if changing the vote.
#[payable]
pub fn vote(
    &mut self,
    proposal_id: ProposalId,
    vote: u8,
    merkle_proof: MerkleProof,
    v_account: VAccount,
);

/// Returns the vote of the given account ID and proposal ID.
pub fn get_vote(&self, account_id: AccountId, proposal_id: ProposalId) -> Option<u8>;
```
