use near_sdk::store::LookupMap;
use near_sdk::{near, AccountId, BorshStorageKey, PanicOnDefault};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Yolo,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    yolo: LookupMap<AccountId, AccountId>,
}

#[near]
impl Contract {
    #[init]
    pub fn init() -> Self {
        Self {
            yolo: LookupMap::new(StorageKeys::Yolo),
        }
    }
}
