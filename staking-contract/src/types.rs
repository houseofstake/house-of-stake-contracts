//! Catalog, lifecycle, and versioned on-chain storage types.

use crate::config::Config;
pub use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
#[cfg(test)]
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{U64, U128};
use near_sdk::{AccountId, NearToken, near};

/// Stripe-style string IDs (generated in [`crate::ids`]).
pub type ProductId = String;
pub type PriceId = String;
pub type SubscriptionId = String;
pub type LockId = String;
pub type PurchaseId = String;

/// Staking pool contract account id (allowlist key, catalog scope, lock pool).
pub type ValidatorId = AccountId;

/// Snapshot from a NEAR staking pool `get_account` view (matches `HumanReadableAccount` fields used here).
#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct PoolAccountView {
    pub unstaked_balance: U128,
    pub staked_balance: U128,
    /// Pool-side unstaked unlock flag (`unstaked_available_epoch_height <= epoch_height`).
    pub can_withdraw: bool,
}

impl PoolAccountView {
    pub fn unstaked(&self) -> NearToken {
        NearToken::from_yoctonear(self.unstaked_balance.0)
    }

    pub fn total_balance(&self) -> NearToken {
        NearToken::from_yoctonear(
            self.unstaked_balance
                .0
                .saturating_add(self.staked_balance.0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum PriceType {
    OneOff,
    Recurring,
    Farm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum BillingPeriod {
    Monthly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum CatalogStatus {
    Active,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum SubscriptionStatus {
    /// Current billing period is valid.
    Active,
    /// Subscription was terminated before its natural period end.
    Cancelled,
    /// Subscription reached its period end and did not renew.
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum ValidatorStatus {
    Active,
    Paused,
    Removed,
}

/// Same as lockup-contract: serial async pool operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum TransactionStatus {
    Idle,
    Busy,
}

/// NEP-145-style prepaid storage for a registered user.
#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Account {
    pub storage_deposit: NearToken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct AccountView {
    pub account_id: AccountId,
    pub storage_deposit: NearToken,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            storage_deposit: NearToken::from_near(0),
        }
    }
}

/// Allowlisted staking pool row (pool contract account id + share-pool accounting).
#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Validator {
    /// Staking pool contract account (= catalog `validator_id` / lock `validator_id`).
    pub validator_id: ValidatorId,
    /// Optional bounded list of catalog managers allowed to manage products and prices for this validator.
    pub catalog_manager_account_ids: Vec<AccountId>,
    /// Whether new locks are allowed (**`Active`**) or blocked (**`Paused`**), or this pool is **`Removed`**.
    pub status: ValidatorStatus,

    /// Total issued stake.dao **share units** for this pool (integer; same scale as per-user shares).
    pub total_shares: U128,
    /// Cached **staked NEAR** for this contract’s account on the pool, from the last successful
    /// pool `get_account` total balance (plus bookkeeping updates on stake/unstake/withdraw paths). Used with
    /// `pending_*` for share mint/burn pricing—not updated until the next pool read or accounting step.
    pub total_staked_balance: NearToken,
    /// `block_timestamp` (nanoseconds) when `total_staked_balance` was last synced from the pool (or validator was added).
    pub last_balance_refresh_ns: U64,

    /// NEAR waiting to be sent to the pool via **`deposit_and_stake`** (aggregated locks; net-settled vs
    /// `pending_to_unstake` in `Contract::try_epoch_stake_or_unstake` in `epoch.rs`).
    pub pending_to_stake: NearToken,
    /// NEAR queued to leave the pool via **`unstake`** (user unlocks etc.; net-settled vs `pending_to_stake`).
    pub pending_to_unstake: NearToken,
    /// Epoch height recorded after the last successful pool `unstake` callback; gates further unstakes
    /// (with [`Config::epoch_unstake_settle_epochs`]).
    pub last_unstake_epoch: u64,
    /// Last NEAR `epoch_height` for which this validator completed the **pre–user-action** pipeline for a request:
    /// **sync** `total_staked_balance` from the pool (at most once per epoch for catalog flows), **withdraw**
    /// from the pool when eligible, then at most one **net** pool `deposit_and_stake` / `unstake` / net-zero
    /// clearance for that epoch (same mutex the staking pool enforces per account). Successful callbacks
    /// on stake, unstake, net-zero settlement, no-pending settlement, and unstake-waiting
    /// settlement decisions set this to `env::epoch_height()`. When this equals the current epoch,
    /// user flows **skip** another pool `get_account` refresh for this validator until the next
    /// NEAR epoch.
    pub last_settlement_epoch: u64,
    /// NEAR that has been unstaked on the pool side and is expected to be moved by pool `withdraw`
    /// into this contract.
    pub pending_to_withdraw: NearToken,
    /// NEAR already moved from pool into this contract and now claimable by users.
    pub pending_to_claim: NearToken,

    /// At most one in-flight cross-contract **mutating** pool pipeline for this validator (`Idle` vs `Busy`).
    pub tx_status: TransactionStatus,
}

impl Validator {
    /// Total user-exit liability across all buckets.
    pub fn pending_user_liability_yocto(&self) -> u128 {
        self.pending_to_unstake
            .as_yoctonear()
            .saturating_add(self.pending_to_withdraw.as_yoctonear())
            .saturating_add(self.pending_to_claim.as_yoctonear())
    }

    /// Liability still outside this contract (pool stake + pool unstaked).
    pub fn pending_not_in_contract_yocto(&self) -> u128 {
        self.pending_to_unstake
            .as_yoctonear()
            .saturating_add(self.pending_to_withdraw.as_yoctonear())
    }

    pub fn gross_stake_yocto(&self) -> u128 {
        self.total_staked_balance
            .as_yoctonear()
            .saturating_add(self.pending_to_stake.as_yoctonear())
    }

    /// NEAR backing **remaining** circulating shares: gross effective stake minus user liability that
    /// is still outside this contract (`pending_to_unstake + pending_to_withdraw`).
    ///
    /// **Solvency:** share pricing must not use gross backing alone after shares burn down.
    /// `pending_to_claim` is already in-contract cash and should not reduce pool-side backing again.
    pub fn net_stake_yocto(&self) -> u128 {
        self.gross_stake_yocto()
            .saturating_sub(self.pending_not_in_contract_yocto())
    }
}

/// One slice of a user's post-unlock NEAR liability; claims from `pending_to_claim` are allowed
/// when `env::epoch_height() >= available_epoch_height` (see [`crate::Contract::pending_unstake_tranche_available_epoch_height`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub struct PendingUnstakeTranche {
    pub amount: NearToken,
    /// Earliest `epoch_height` (inclusive) for [`crate::Contract::withdraw`]; set at unlock via
    /// [`crate::Contract::pending_unstake_tranche_available_epoch_height`].
    pub available_epoch_height: u64,
}

/// UI-ready pending unstake and withdraw status for one `(account, validator)` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct AccountPendingUnstakeView {
    /// Account whose pending unstake tranches are being queried.
    pub account_id: AccountId,
    /// Validator pool account for this pending unstake view.
    pub validator_id: ValidatorId,
    /// Current NEAR epoch height used to evaluate tranche eligibility.
    pub current_epoch_height: u64,
    /// Configured epoch wait used by unstake scheduling and UI wait estimates.
    pub epoch_unstake_settle_epochs: u64,
    /// Sum of all pending unstake tranches for this account and validator, including future epochs.
    pub total_pending_yocto: U128,
    /// Sum of tranches whose epoch wait has completed (`current_epoch_height >= available_epoch_height`).
    pub epoch_eligible_yocto: U128,
    /// Amount the user can withdraw now; zero unless every epoch-eligible tranche can be paid.
    pub withdrawable_yocto: U128,
    /// Earliest future tranche availability epoch, or `None` when no future tranches exist.
    pub next_available_epoch_height: Option<u64>,
    /// Epochs until `next_available_epoch_height`, or `None` when no future tranches exist.
    pub wait_epochs: Option<u64>,
    /// Validator claim bucket currently available to pay user withdrawals from this contract.
    pub pending_to_claim_yocto: U128,
    /// True only when `withdraw` should be enabled for this account and validator.
    pub can_withdraw_now: bool,
    /// Per-tranche pending unstake details in storage order.
    pub tranches: Vec<PendingUnstakeTrancheView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct PendingUnstakeTrancheView {
    /// Tranche amount in yoctoNEAR.
    pub amount_yocto: U128,
    /// Earliest NEAR epoch height where this tranche can be claimed by `withdraw`.
    pub available_epoch_height: u64,
    /// Whether this tranche's epoch wait has completed at `AccountPendingUnstakeView::current_epoch_height`.
    pub is_epoch_eligible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum LockStatus {
    Active,
    UnlockRequested,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum FarmStatus {
    Active,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub enum OrderRef {
    ProductPurchase {
        product_id: ProductId,
        price_id: PriceId,
    },
    Subscription {
        subscription_id: SubscriptionId,
        price_id: PriceId,
        period_start_ns: U64,
        period_end_ns: U64,
    },
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Product {
    pub product_id: ProductId,
    pub validator_id: ValidatorId,
    pub name: String,
    pub description: String,
    pub status: CatalogStatus,
    pub created_ns: U64,
    pub price_ids: Vec<PriceId>,
    /// Default catalog price (`price_*`) for **`lock`** when called with **`product_id`** and **`price_id: null`** (see **`set_product_default_price`**).
    /// Only an **active** (unarchived) price id may be stored; archiving the product/price clears this when it matches.
    pub default_price_id: Option<PriceId>,
    pub usage_count: u64,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct PriceMetadata {
    /// Optional inclusive upper bound for variable subscription stake amounts and active farm stake.
    pub max_amount: Option<U128>,
    /// Farm-only reward rate. Unit: 24-decimal reward units per second per staked NEAR,
    /// scaled by [`crate::stake::FARM_REWARD_RATE_DENOM`].
    pub farm_reward_rate: Option<U128>,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Price {
    pub price_id: PriceId,
    pub product_id: ProductId,
    pub name: String,
    pub description: String,
    /// NEAR amount in yoctoNEAR (catalog denomination).
    pub amount: U128,
    pub price_type: PriceType,
    pub billing_period: Option<BillingPeriod>,
    /// Fixed-point lock-weight; see [`crate::utils::LOCK_FACTOR_DENOM`] and [`crate::utils::check_near_price_lock`].
    pub lock_factor_near_months: U128,
    /// Optional metadata for price-specific constraints. For recurring subscription prices,
    /// `max_amount` is the inclusive upper bound for the selected stake amount.
    pub metadata: Option<PriceMetadata>,
    pub status: CatalogStatus,
    pub usage_count: u64,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Subscription {
    pub subscription_id: SubscriptionId,
    pub account_id: AccountId,
    pub product_id: ProductId,
    pub price_id: PriceId,
    pub start_ns: U64,
    pub end_ns: U64,
    pub anchor_day: u8,
    pub last_lock_id: LockId,
    pub status: SubscriptionStatus,
    /// When true, no renewal after the current billing period (`end_ns`); lock still runs until unlock.
    pub cancel_at_period_end: bool,
    /// Deferred plan and/or stake decrease to apply at a future billing boundary.
    pub pending_update: Option<PendingSubscriptionUpdate>,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct PendingSubscriptionUpdate {
    /// Target plan to apply at `apply_ns`. Absent when only stake amount decreases.
    pub target_price_id: Option<PriceId>,
    /// Target stake amount to apply at `apply_ns`. Absent when only plan changes.
    pub target_amount: Option<NearToken>,
    /// Timestamp when the pending update becomes effective.
    pub apply_ns: U64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct SubscriptionPlanChangeOutcome {
    pub kind: String,
    pub subscription_id: SubscriptionId,
    pub target_price_id: PriceId,
    pub target_amount: U128,
    pub lock_id: Option<LockId>,
    pub immediate_plan_change: bool,
    pub immediate_stake_increase: Option<U128>,
    pub pending_plan_change: bool,
    pub pending_stake_decrease: Option<U128>,
    pub pending_apply_ns: Option<U64>,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Lock {
    pub lock_id: LockId,
    pub account_id: AccountId,
    pub validator_id: ValidatorId,
    pub amount_near: NearToken,
    pub shares: U128,
    pub start_ns: U64,
    pub end_ns: U64,
    pub order: OrderRef,
    pub status: LockStatus,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct Purchase {
    pub purchase_id: PurchaseId,
    pub account_id: AccountId,
    pub product_id: ProductId,
    pub price_id: PriceId,
    pub quantity: U64,
    pub amount_paid: NearToken,
    pub created_ns: U64,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct FarmPool {
    /// Farm price that owns this accumulator.
    pub price_id: PriceId,
    /// Product for deriving the validator and product-level catalog constraints.
    pub product_id: ProductId,
    /// 24-decimal reward units emitted per second per staked NEAR.
    pub reward_rate: U128,
    /// Validator shares currently attributed to active farm positions for this price.
    pub total_farm_shares: U128,
    /// Cumulative reward units per farm share, scaled by `FARM_ACC_REWARD_PER_SHARE_DENOM`.
    pub acc_reward_per_share: U128,
    /// Last timestamp when this accumulator was settled.
    pub last_reward_settle_ns: U64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [borsh, json])]
pub struct FarmPosition {
    /// Position owner.
    pub account_id: AccountId,
    /// Product with one current or historical farm position for this account.
    pub product_id: ProductId,
    /// Farm price whose accumulator this position follows.
    pub price_id: PriceId,
    /// Denormalized validator used by immutable historical position views.
    pub validator_id: ValidatorId,
    /// Validator shares currently active for this farm position.
    pub shares: U128,
    /// Position's share of `FarmPool.acc_reward_per_share` already accounted for.
    pub reward_debt: U128,
    /// Settled but not yet rolled up reward units for this position.
    pub accrued_reward_units: U128,
    /// Active positions earn rewards; closed positions remain for historical views.
    pub status: FarmStatus,
    /// Position creation or last reopen timestamp.
    pub created_ns: U64,
    /// Last stake, unstake, or reward-settlement timestamp.
    pub updated_ns: U64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct FarmPositionView {
    /// Position owner.
    pub account_id: AccountId,
    /// Product with one current or historical farm position for this account.
    pub product_id: ProductId,
    /// Farm price whose accumulator this position follows.
    pub price_id: PriceId,
    /// Denormalized validator used by immutable historical position views.
    pub validator_id: ValidatorId,
    /// Validator shares currently active for this farm position.
    pub shares: U128,
    /// Active staked NEAR amount represented by `shares` at view time.
    pub staked_near_amount: U128,
    /// Position's share of `FarmPool.acc_reward_per_share` already accounted for.
    pub reward_debt: U128,
    /// Settled but not yet rolled up reward units for this position.
    pub accrued_reward_units: U128,
    /// Simulated pending rewards for this position, including settled-but-unrolled rewards.
    pub pending_reward_units: U128,
    /// Total position rewards not yet rolled into the farm account.
    pub total_earned_reward_units: U128,
    /// Active positions earn rewards; closed positions remain for historical views.
    pub status: FarmStatus,
    /// Position creation or last reopen timestamp.
    pub created_ns: U64,
    /// Last stake, unstake, or reward-settlement timestamp.
    pub updated_ns: U64,
}

#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub struct FarmAccount {
    /// Account that owns the closed-position reward roll-up.
    pub account_id: AccountId,
    /// Closed-position reward roll-up already earned by this account.
    pub accumulated_reward_units: U128,
    /// Number of active farm positions with non-zero shares.
    pub active_position_count: u32,
    pub last_update_ns: U64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[near(serializers = [json])]
pub struct FarmAccountView {
    /// Account that owns this farm reward roll-up.
    pub account_id: AccountId,
    /// Stored rewards from farm positions that have been fully closed.
    pub accumulated_reward_units: U128,
    /// Simulated pending rewards across currently active farm positions.
    pub pending_reward_units: U128,
    /// `accumulated_reward_units + pending_reward_units`.
    pub total_earned_reward_units: U128,
    /// Active farm positions with non-zero shares at view time.
    pub active_positions: Vec<FarmPositionView>,
}

/// User-facing tail chained after [`Contract::promise_validator_per_epoch_settlement_then`]:
/// either the full pre-user pipeline ran (balance sync → withdraw-if-ready →
/// [`crate::epoch::Contract::try_epoch_stake_or_unstake`]), or the pool had **already** settled this NEAR epoch and
/// the contract skipped that pipeline and jumped straight here (cached **`total_staked_balance`**).
#[derive(Clone)]
#[near(serializers = [borsh, json])]
pub enum UserAction {
    /// Mint a catalog lock after settlement (`lock`).
    CommitLock {
        validator_id: ValidatorId,
        buyer: AccountId,
        locked: NearToken,
        duration_ns: u128,
        order: OrderRef,
    },
    /// Recurring subscription lock resolved after the validator settlement preamble,
    /// before subscription renewal or new-period state is committed.
    CommitRecurringSubscriptionLock {
        validator_id: ValidatorId,
        buyer: AccountId,
        locked: NearToken,
        price_id: PriceId,
    },
    UnlockQueueUnstake {
        validator_id: ValidatorId,
        lock_id: LockId,
        account_id: AccountId,
        shares_remove: u128,
    },
    /// After shared per-epoch settlement: batch claim + NEAR transfer for [`crate::Contract::withdraw`].
    WithdrawUserTransfer {
        validator_id: ValidatorId,
        account_id: AccountId,
    },
    /// Public [`crate::epoch::Contract::epoch_settle`]: no user tail after the shared pipeline.
    SettleOnly { validator_id: ValidatorId },
    /// Mid-period subscription update after pre-user settlement (`subscriptions.rs`).
    SubscriptionUpdate {
        validator_id: ValidatorId,
        buyer: AccountId,
        deposit: NearToken,
        target_price_id: PriceId,
        target_amount: U128,
        subscription_id: SubscriptionId,
    },
    CommitFarmStake {
        validator_id: ValidatorId,
        account_id: AccountId,
        deposit: NearToken,
        product_id: ProductId,
        price_id: PriceId,
    },
    FarmUnstakeQueue {
        validator_id: ValidatorId,
        account_id: AccountId,
        product_id: ProductId,
        amount: Option<U128>,
    },
}

impl UserAction {
    pub fn validator_id(&self) -> &ValidatorId {
        match self {
            Self::CommitLock { validator_id, .. }
            | Self::CommitRecurringSubscriptionLock { validator_id, .. }
            | Self::UnlockQueueUnstake { validator_id, .. }
            | Self::WithdrawUserTransfer { validator_id, .. }
            | Self::SettleOnly { validator_id }
            | Self::SubscriptionUpdate { validator_id, .. }
            | Self::CommitFarmStake { validator_id, .. }
            | Self::FarmUnstakeQueue { validator_id, .. } => validator_id,
        }
    }

    /// NEAR attached on the entry receipt for payable flows (`lock`, `update_subscription`).
    /// Used to refund when the async pre-user pipeline aborts before the user-flow tail commits.
    pub fn payable_refund(&self) -> Option<(AccountId, NearToken)> {
        match self {
            Self::CommitLock { buyer, locked, .. } => Some((buyer.clone(), *locked)),
            Self::CommitRecurringSubscriptionLock { buyer, locked, .. } => {
                Some((buyer.clone(), *locked))
            }
            Self::SubscriptionUpdate { buyer, deposit, .. } => Some((buyer.clone(), *deposit)),
            Self::CommitFarmStake {
                account_id,
                deposit,
                ..
            } => Some((account_id.clone(), *deposit)),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Launch borsh wrappers (`LookupMap` values and `Contract.config`)
//
// These wrappers keep one explicit envelope around stored rows so future post-launch
// migrations can add variants intentionally. Pre-launch historical variants were removed
// because production accounts start from empty state.
//
// [`VConfig`]: [`AsRef`] / [`AsMut`] for in-place `Contract.config` access. Map values:
// [`From`] / `into()` only.
// -----------------------------------------------------------------------------

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VConfig {
    Current(Config),
}

impl From<Config> for VConfig {
    fn from(value: Config) -> Self {
        Self::Current(value)
    }
}

impl From<VConfig> for Config {
    fn from(value: VConfig) -> Self {
        match value {
            VConfig::Current(inner) => inner,
        }
    }
}

impl AsRef<Config> for VConfig {
    fn as_ref(&self) -> &Config {
        match self {
            VConfig::Current(c) => c,
        }
    }
}

impl AsMut<Config> for VConfig {
    fn as_mut(&mut self) -> &mut Config {
        match self {
            VConfig::Current(c) => c,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VValidator {
    Current(Validator),
}

impl From<Validator> for VValidator {
    fn from(value: Validator) -> Self {
        Self::Current(value)
    }
}

impl From<VValidator> for Validator {
    fn from(value: VValidator) -> Self {
        match value {
            VValidator::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VProduct {
    Current(Product),
}

impl From<Product> for VProduct {
    fn from(value: Product) -> Self {
        Self::Current(value)
    }
}

impl From<VProduct> for Product {
    fn from(value: VProduct) -> Self {
        match value {
            VProduct::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VPrice {
    Current(Price),
}

impl From<Price> for VPrice {
    fn from(value: Price) -> Self {
        Self::Current(value)
    }
}

impl From<VPrice> for Price {
    fn from(value: VPrice) -> Self {
        match value {
            VPrice::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VAccount {
    Current(Account),
}

impl From<Account> for VAccount {
    fn from(value: Account) -> Self {
        Self::Current(value)
    }
}

impl From<VAccount> for Account {
    fn from(value: VAccount) -> Self {
        match value {
            VAccount::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VSubscription {
    Current(Subscription),
}

impl From<Subscription> for VSubscription {
    fn from(value: Subscription) -> Self {
        Self::Current(value)
    }
}

impl From<VSubscription> for Subscription {
    fn from(value: VSubscription) -> Self {
        match value {
            VSubscription::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VLock {
    Current(Lock),
}

impl From<Lock> for VLock {
    fn from(value: Lock) -> Self {
        Self::Current(value)
    }
}

impl From<VLock> for Lock {
    fn from(value: VLock) -> Self {
        match value {
            VLock::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VPurchase {
    Current(Purchase),
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VFarmPool {
    Current(FarmPool),
}

impl From<FarmPool> for VFarmPool {
    fn from(value: FarmPool) -> Self {
        Self::Current(value)
    }
}

impl From<VFarmPool> for FarmPool {
    fn from(value: VFarmPool) -> Self {
        match value {
            VFarmPool::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VFarmPosition {
    Current(FarmPosition),
}

impl From<FarmPosition> for VFarmPosition {
    fn from(value: FarmPosition) -> Self {
        Self::Current(value)
    }
}

impl From<VFarmPosition> for FarmPosition {
    fn from(value: VFarmPosition) -> Self {
        match value {
            VFarmPosition::Current(inner) => inner,
        }
    }
}

#[derive(Clone)]
#[near(serializers = [borsh])]
pub enum VFarmAccount {
    Current(FarmAccount),
}

impl From<FarmAccount> for VFarmAccount {
    fn from(value: FarmAccount) -> Self {
        Self::Current(value)
    }
}

impl From<VFarmAccount> for FarmAccount {
    fn from(value: VFarmAccount) -> Self {
        match value {
            VFarmAccount::Current(inner) => inner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn farm_price() -> Price {
        Price {
            price_id: "price_farm".to_string(),
            product_id: "product_farm".to_string(),
            name: "Farm".to_string(),
            description: "Farm price".to_string(),
            amount: U128(1),
            price_type: PriceType::Farm,
            billing_period: None,
            lock_factor_near_months: U128(0),
            metadata: Some(PriceMetadata {
                max_amount: Some(U128(100)),
                farm_reward_rate: Some(U128(7)),
            }),
            status: CatalogStatus::Active,
            usage_count: 1,
        }
    }

    fn validator() -> Validator {
        Validator {
            validator_id: "pool.near".parse().unwrap(),
            catalog_manager_account_ids: vec!["manager.near".parse().unwrap()],
            status: ValidatorStatus::Active,
            total_shares: U128(10),
            total_staked_balance: NearToken::from_near(10),
            last_balance_refresh_ns: U64(1),
            pending_to_stake: NearToken::from_near(2),
            pending_to_unstake: NearToken::from_near(3),
            last_unstake_epoch: 4,
            last_settlement_epoch: 5,
            pending_to_withdraw: NearToken::from_near(6),
            pending_to_claim: NearToken::from_near(7),
            tx_status: TransactionStatus::Idle,
        }
    }

    #[test]
    fn vprice_writes_launch_prices_as_current_variant() {
        let price = farm_price();
        let mut bytes = Vec::new();
        VPrice::from(price.clone()).serialize(&mut bytes).unwrap();

        assert_eq!(bytes.first().copied(), Some(0));
        let decoded: Price = VPrice::try_from_slice(&bytes).unwrap().into();
        assert_eq!(decoded.price_id, price.price_id);
        assert_eq!(
            decoded.metadata.as_ref().unwrap().farm_reward_rate,
            Some(U128(7))
        );
    }

    #[test]
    fn vvalidator_writes_launch_validators_as_current_variant() {
        let mut bytes = Vec::new();
        VValidator::from(validator()).serialize(&mut bytes).unwrap();

        assert_eq!(bytes.first().copied(), Some(0));
        let decoded = VValidator::try_from_slice(&bytes).unwrap();
        let decoded: Validator = decoded.into();
        assert_eq!(decoded.validator_id.as_str(), "pool.near");
        assert_eq!(
            decoded.catalog_manager_account_ids,
            vec!["manager.near".parse::<AccountId>().unwrap()]
        )
    }
}

impl From<Purchase> for VPurchase {
    fn from(value: Purchase) -> Self {
        Self::Current(value)
    }
}

impl From<VPurchase> for Purchase {
    fn from(value: VPurchase) -> Self {
        match value {
            VPurchase::Current(inner) => inner,
        }
    }
}

#[cfg(test)]
mod versioned_tests {
    use super::*;
    use near_sdk::json_types::U64;

    #[test]
    fn vconfig_current_roundtrip() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let cfg = Config {
            owner_account_id: owner.clone(),
            proposed_new_owner_account_id: None,
            guardians: vec![],
            min_lock_duration_ns: U64(1),
            max_lock_duration_ns: U64(2),
            epoch_unstake_settle_epochs: 4,
            min_storage_deposit: NearToken::from_near(1),
            per_lock_storage_stake: NearToken::from_near(0),
            per_farm_position_storage_stake: NearToken::from_near(0),
            per_purchase_storage_stake: NearToken::from_near(0),
            min_lock_amount: NearToken::from_near(1),
        };
        let v: VConfig = cfg.clone().into();
        let back: Config = v.into();
        assert_eq!(back.owner_account_id, owner);
    }
}
