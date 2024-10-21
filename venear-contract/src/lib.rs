use merkle_tree::MerkleTree;

use near_sdk::store::LookupMap;
use near_sdk::{near, AccountId, BorshStorageKey, PanicOnDefault};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Yolo,
    Accounts,
}

#[near(serializers=[borsh, json])]
pub struct Account {}

#[near(serializers=[borsh, json])]
pub enum VAccount {
    Current(Account),
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    yolo: LookupMap<AccountId, AccountId>,
    accounts: MerkleTree<VAccount>,
}

#[near]
impl Contract {
    #[init]
    pub fn init() -> Self {
        Self {
            yolo: LookupMap::new(StorageKeys::Yolo),
            accounts: MerkleTree::new(StorageKeys::Accounts),
        }
    }
}
