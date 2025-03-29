use near_sdk::store::{LookupMap, LookupSet};
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, Promise,
};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Whitelist,
    Accounts,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    whitelist: LookupSet<AccountId>,
    accounts: LookupMap<AccountId, Account>,
}

#[derive(Debug, Clone)]
#[near(serializers=[borsh, json])]
pub struct Account {
    pub staked_balance: NearToken,
    pub unstaked_balance: NearToken,
    pub can_withdraw: bool,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            staked_balance: NearToken::default(),
            unstaked_balance: NearToken::default(),
            can_withdraw: true,
        }
    }
}

#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {
            whitelist: LookupSet::new(StorageKeys::Whitelist),
            accounts: LookupMap::new(StorageKeys::Accounts),
        }
    }

    /// Fake method for testing
    pub fn sandbox_whitelist(&mut self, staking_pool_account_id: AccountId) {
        self.whitelist.insert(staking_pool_account_id);
    }

    /// Called by lockup contract to check whitelist of a staking pool
    pub fn is_whitelisted(&self, staking_pool_account_id: AccountId) -> bool {
        self.whitelist.contains(&staking_pool_account_id)
    }

    // Methods for staking pool

    /// Fake method for testing
    pub fn sandbox_update_account(&mut self, account_id: AccountId, account: Account) {
        self.accounts.insert(account_id, account);
    }

    pub fn get_account(&self, account_id: &AccountId) -> Option<Account> {
        self.accounts.get(account_id).cloned()
    }

    pub fn get_account_staked_balance(&self, account_id: AccountId) -> NearToken {
        self.get_account(&account_id)
            .map(|account| account.staked_balance)
            .unwrap_or_default()
    }

    pub fn get_account_unstaked_balance(&self, account_id: AccountId) -> NearToken {
        self.get_account(&account_id)
            .map(|account| account.unstaked_balance)
            .unwrap_or_default()
    }

    pub fn get_account_total_balance(&self, account_id: AccountId) -> NearToken {
        self.get_account(&account_id)
            .map(|account| {
                account
                    .staked_balance
                    .checked_add(account.unstaked_balance)
                    .unwrap()
            })
            .unwrap_or_default()
    }

    #[payable]
    pub fn deposit(&mut self) {
        let attached_deposit = env::attached_deposit();
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        account.unstaked_balance = account
            .unstaked_balance
            .checked_add(attached_deposit.into())
            .unwrap();
        self.accounts.insert(account_id, account);
    }

    #[payable]
    pub fn deposit_and_stake(&mut self) {
        let attached_deposit = env::attached_deposit();
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        account.staked_balance = account
            .staked_balance
            .checked_add(attached_deposit.into())
            .unwrap();
        account.can_withdraw = false;
        self.accounts.insert(account_id, account);
    }

    pub fn withdraw(&mut self, amount: NearToken) {
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        require!(
            account.can_withdraw,
            "You cannot withdraw until the lockup period is over"
        );

        require!(
            account.unstaked_balance >= amount,
            "Not enough unstaked balance"
        );

        account.unstaked_balance = account.unstaked_balance.checked_sub(amount).unwrap();
        Promise::new(account_id.clone()).transfer(amount);
        self.accounts.insert(account_id, account);
    }

    pub fn stake(&mut self, amount: NearToken) {
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        require!(
            account.unstaked_balance >= amount,
            "Not enough unstaked balance"
        );

        account.unstaked_balance = account.unstaked_balance.checked_sub(amount).unwrap();
        account.staked_balance = account.staked_balance.checked_add(amount).unwrap();
        account.can_withdraw = false;
        self.accounts.insert(account_id, account);
    }

    pub fn unstake(&mut self, amount: NearToken) {
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        require!(
            account.staked_balance >= amount,
            "Not enough staked balance"
        );

        account.staked_balance = account.staked_balance.checked_sub(amount).unwrap();
        account.unstaked_balance = account.unstaked_balance.checked_add(amount).unwrap();
        account.can_withdraw = false;
        self.accounts.insert(account_id, account);
    }

    pub fn unstake_all(&mut self) {
        let account_id = env::predecessor_account_id();
        let mut account = self.get_account(&account_id).unwrap_or_default();

        account.unstaked_balance = account
            .unstaked_balance
            .checked_add(account.staked_balance)
            .unwrap();
        account.staked_balance = NearToken::default();
        account.can_withdraw = false;
        self.accounts.insert(account_id, account);
    }
}
