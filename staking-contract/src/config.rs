use crate::*;
use near_sdk::json_types::U64;
use near_sdk::{AccountId, NearToken, near, require};

/// Minimum allowed [`Config::min_lock_amount`] in yoctoNEAR (1 NEAR). Stake pools reject first
/// delegations below this scale; the contract enforces the same floor for config and new deploys.
pub const PROTOCOL_MIN_LOCK_AMOUNT_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

/// Panics unless `min_lock_amount` is at least [`PROTOCOL_MIN_LOCK_AMOUNT_YOCTO`].
pub fn require_min_lock_amount_at_protocol_floor(min_lock_amount: &NearToken) {
    require!(
        min_lock_amount.as_yoctonear() >= PROTOCOL_MIN_LOCK_AMOUNT_YOCTO,
        "min_lock_amount must be at least 1 NEAR (stake pool delegation minimum)"
    );
}

#[derive(Clone, Debug)]
#[near(serializers = [borsh, json])]
pub struct Config {
    pub owner_account_id: AccountId,
    pub proposed_new_owner_account_id: Option<AccountId>,
    pub guardians: Vec<AccountId>,
    pub min_lock_duration_ns: U64,
    pub max_lock_duration_ns: U64,
    pub epoch_unstake_settle_epochs: u64,
    pub min_storage_deposit: NearToken,
    /// Per lock record ever created (see [`crate::Contract::user_lock_count`]). Zero disables the
    /// extra requirement beyond [`Self::min_storage_deposit`].
    pub per_lock_storage_stake: NearToken,
    /// Per farm-position record ever created (see [`crate::Contract::user_farm_position_count`]).
    /// Zero disables the extra requirement beyond [`Self::min_storage_deposit`].
    pub per_farm_position_storage_stake: NearToken,
    /// Per **direct purchase ever created** (see [`crate::Contract::user_purchase_count`]); zero disables
    /// the extra requirement beyond [`Self::min_storage_deposit`].
    pub per_purchase_storage_stake: NearToken,
    /// Minimum NEAR attached for locks and subscription payments. Governance cannot set this below
    /// [`PROTOCOL_MIN_LOCK_AMOUNT_YOCTO`] (see [`require_min_lock_amount_at_protocol_floor`]).
    pub min_lock_amount: NearToken,
}

#[near]
impl Contract {
    pub fn get_config(&self) -> &Config {
        self.internal_get_config()
    }
}

impl Contract {
    pub(crate) fn internal_get_config(&self) -> &Config {
        self.config.as_ref()
    }

    pub(crate) fn internal_get_config_mut(&mut self) -> &mut Config {
        self.config.as_mut()
    }
}
