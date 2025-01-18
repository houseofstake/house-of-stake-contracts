use merkle_tree::{MerkleProof, MerkleTree, MerkleTreeSnapshot};

use common::account::*;
use common::global_state::*;
use near_sdk::{
    near,
    serde::{Deserialize, Serialize},
    AccountId, BorshStorageKey, PanicOnDefault,
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct AccountInfo {
    pub account_id: AccountId,
    // todo: add more fields
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

    #[payable]
    pub fn register_account(&mut self) {
        todo!()
    }

    pub fn get_registration_cost(&self) -> u128 {
        todo!()
    }

    pub fn get_account_info(&self, account_id: AccountId) -> Option<AccountInfo> {
        todo!()
    }

    pub fn get_snapshot(&self) -> (VGlobalState, MerkleTreeSnapshot) {
        todo!()
    }

    pub fn get_proof(&self, account_id: AccountId) -> (VAccount, MerkleProof) {
        todo!()
    }

    // TODO: delegations
    // TODO: veNEAR token non-transferable implementation
}
