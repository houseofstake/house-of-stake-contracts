mod account;
mod config;
mod delegation;
mod lockup;
mod snapshot;

use merkle_tree::{MerkleProof, MerkleTree, MerkleTreeSnapshot};

use common::account::*;
use common::global_state::*;
use near_sdk::{
    near,
    serde::{Deserialize, Serialize},
    AccountId, BorshStorageKey, NearToken, PanicOnDefault,
};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Tree,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    tree: MerkleTree<VAccount, VGlobalState>,
}

#[near]
impl Contract {
    #[init]
    pub fn init() -> Self {
        Self {
            tree: MerkleTree::new(StorageKeys::Tree, GlobalState::new().into()),
        }
    }

    //TODO
    // Flow
    // 1. Check if account exists (get_account_info)
    // 2. If not, create new account (attempts to deploys a new lockup. Keeps some funds for local storage)
    //
    // veNEAR implementation:
    // - Get veNear balance
    //
    // Internal:
    // - update veNear from lockup, e.g. unlocking, relocking.
    // - internal configuration
    // - contract upgrades
    //
    // Making snapshots for new voting
    //
    // Delegation of veNEAR
    //
    // Voting
    // Lockup integration on account update

    // TODO: delegations
    // TODO: veNEAR token non-transferable implementation
}
