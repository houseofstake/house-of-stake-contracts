use crate::*;
use common::Version;
use near_sdk::json_types::{Base58CryptoHash, U64};

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct LockupContractConfig {
    pub contract_size: u64,
    pub contract_version: Version,
    pub contract_hash: Base58CryptoHash,
}

#[derive(Clone)]
#[near(serializers=[json, borsh])]
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

    /// The minimum additional amount in NEAR required for lockup deployment.
    pub min_extra_lockup_deposit: NearToken,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,
}

impl Contract {
    pub fn internal_get_venear_growth_config(&self) -> &VenearGrowthConfig {
        self.tree.get_global_state().get_venear_growth_config()
    }
}
