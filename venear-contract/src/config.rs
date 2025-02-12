use crate::*;
use common::Version;
use near_sdk::json_types::U64;

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct LockupContractConfig {
    pub contract_size: u64,
    pub contract_version: Version,
    pub contract_hash: CryptoHash,
}

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct Config {
    pub lockup_contract_config: LockupContractConfig,

    /// Initialization arguments for the lockup contract.
    pub lockup_duration_ns: U64,
    /// The account ID of the staking pool whitelist for lockup contract.
    pub staking_pool_whitelist_account_id: AccountId,

    /// The list of account IDs that can store new lockup contract code.
    pub lockup_code_deployers: Vec<AccountId>,
}
