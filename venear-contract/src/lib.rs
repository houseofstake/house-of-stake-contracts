mod account;
mod config;
mod delegation;
mod global_state;
mod governance;
mod lockup;
mod snapshot;
mod storage;
mod token;
mod upgrade;

use merkle_tree::{MerkleProof, MerkleTree, MerkleTreeSnapshot};

use crate::account::VAccountInternal;
use crate::config::Config;
use common::account::*;
use common::global_state::*;
use common::venear::{VenearGrowthConfig, VenearGrowthConfigFixedRate};
use common::Version;
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, require, sys, AccountId, BorshStorageKey, CryptoHash, NearToken, PanicOnDefault,
};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Tree,
    LockupCode(CryptoHash),
    Accounts,
    Lsts,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    tree: MerkleTree<VAccount, VGlobalState>,
    accounts: LookupMap<AccountId, VAccountInternal>,
    config: Config,
}

#[near]
impl Contract {
    #[init]
    pub fn init(config: Config, venear_growth_config: VenearGrowthConfigFixedRate) -> Self {
        Self {
            tree: MerkleTree::new(
                StorageKeys::Tree,
                GlobalState::new(env::block_timestamp().into(), venear_growth_config.into()).into(),
            ),
            accounts: LookupMap::new(StorageKeys::Accounts),
            config,
        }
    }

    // TODO:
    // - contract upgrades
}
