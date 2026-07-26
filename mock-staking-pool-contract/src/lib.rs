//! Mock staking pool for sandbox / `near-workspaces` tests.
//!
//! **Why this exists:** `staking-contract` calls a real staking pool through `ext_staking_pool`
//! (`staking-contract/src/epoch.rs`). Integration tests deploy this contract at the pool account id so
//! `deposit_and_stake`, `unstake`, `withdraw`, balance views, and catalog `get_owner_id` chains execute as
//! real cross-contract promises instead of host-side mocks.
//!
//! **Surface area:** matches what the staking contract actually invokes — `get_owner_id`,
//! `deposit_and_stake`, `unstake`, `get_account`, `withdraw`. Not a full NEAR core staking-pool implementation.
//!
//! **Accounting (LiNEAR-style split):** for each *caller* account id we track `staked` vs `unstaked`.
//! - `deposit_and_stake` — increases `staked` by `attached_deposit` (caller is the staking contract when
//!   it forwards user deposits).
//! - `unstake` — moves up to `amount` from `staked` to `unstaked` for `predecessor`.
//! - `withdraw` — sends `amount` from `unstaked` to `predecessor` via promise (same pattern as production pools).

use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{
    AccountId, BorshStorageKey, NearToken, PanicOnDefault, Promise, env, near, require,
};

/// Matches staking-contract [`PoolAccountView`] / NEAR staking-pool `get_account` JSON shape.
#[near(serializers = [json])]
pub struct PoolAccountView {
    pub unstaked_balance: U128,
    pub staked_balance: U128,
    pub can_withdraw: bool,
}

#[derive(BorshStorageKey)]
#[near]
enum StorageKey {
    Unstaked,
    Staked,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct MockStakingPoolContract {
    /// Pool operator id returned by `get_owner_id`. Catalog methods on staking compare this to the
    /// transaction signer (`expected_caller`); keep it aligned with who calls `create_product` / etc.
    pub owner_id: AccountId,
    /// NEAR sitting in the “unbonding / liquid on pool” bucket per account (withdraw reads this).
    unstaked: LookupMap<AccountId, NearToken>,
    /// NEAR attributed as staked for each pool participant (here: usually the staking contract id).
    staked: LookupMap<AccountId, NearToken>,
    /// Test hook: force `get_account` to fail while enabled.
    fail_get_account: bool,
    /// Test hook: force next `withdraw` to fail.
    fail_next_withdraw: bool,
}

#[near]
impl MockStakingPoolContract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            unstaked: LookupMap::new(StorageKey::Unstaked),
            staked: LookupMap::new(StorageKey::Staked),
            fail_get_account: false,
            fail_next_withdraw: false,
        }
    }

    /// View used by staking catalog (`products.rs`) and epoch logic to authorize validator-owner actions.
    pub fn get_owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    /// Test-only control: make `get_account` panic while enabled.
    pub fn set_fail_get_account(&mut self, fail: bool) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only pool owner can set test failure flags"
        );
        self.fail_get_account = fail;
    }

    /// Test-only control: make next `withdraw` panic.
    pub fn set_fail_next_withdraw(&mut self, fail: bool) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only pool owner can set test failure flags"
        );
        self.fail_next_withdraw = fail;
    }

    /// Consumes attached NEAR and credits `staked[predecessor]`.
    ///
    /// In production this follows a deposit-to-unstaked-then-stake path; for tests we credit `staked`
    /// directly, which is enough for `on_deposit_and_stake` callbacks on the staking side.
    #[payable]
    pub fn deposit_and_stake(&mut self) {
        let account_id = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        require!(
            deposit.as_yoctonear() > 0,
            "deposit_and_stake requires attached deposit"
        );
        let cur = self.get_staked(&account_id);
        let next = cur.checked_add(deposit).expect("staked overflow");
        self.staked.insert(account_id, next);
    }

    /// Moves up to `amount` from `staked` into `unstaked` for `predecessor` (typically the staking contract).
    pub fn unstake(&mut self, amount: NearToken) {
        let account_id = env::predecessor_account_id();
        let want = amount.as_yoctonear();
        require!(want > 0, "unstake amount must be positive");
        let st = self.get_staked(&account_id);
        let have = st.as_yoctonear();
        let move_yocto = want.min(have);
        require!(move_yocto > 0, "insufficient staked balance");

        let new_staked = NearToken::from_yoctonear(have - move_yocto);
        self.insert_staked(&account_id, new_staked);

        let us = self.get_unstaked(&account_id);
        let new_u = us
            .checked_add(NearToken::from_yoctonear(move_yocto))
            .expect("unstaked overflow");
        self.unstaked.insert(account_id, new_u);
    }

    /// Staked + unstaked snapshot (production pools expose the same via `get_account`).
    pub fn get_account(&self, account_id: AccountId) -> PoolAccountView {
        if self.fail_get_account {
            env::panic_str("mock pool forced get_account failure");
        }
        PoolAccountView {
            unstaked_balance: U128(self.get_unstaked(&account_id).as_yoctonear()),
            staked_balance: U128(self.get_staked(&account_id).as_yoctonear()),
            // Mock pool has no epoch-gated unstaked delay; tests use contract-side gates.
            can_withdraw: true,
        }
    }

    /// Query unstaked bucket for any account (legacy test views).
    pub fn get_account_unstaked_balance(&self, account_id: AccountId) -> NearToken {
        self.get_unstaked(&account_id)
    }

    pub fn get_account_total_balance(&self, account_id: AccountId) -> NearToken {
        let acct = self.get_account(account_id);
        NearToken::from_yoctonear(acct.unstaked_balance.0 + acct.staked_balance.0)
    }

    /// Pulls liquid NEAR from `unstaked[predecessor]` and transfers it to `predecessor` (the staking contract).
    ///
    /// Must return [`Promise`] so the caller’s `.then` callback (`on_epoch_withdraw_transfer_done`) fires.
    pub fn withdraw(&mut self, amount: NearToken) -> Promise {
        if self.fail_next_withdraw {
            self.fail_next_withdraw = false;
            env::panic_str("mock pool forced withdraw failure");
        }
        let account_id = env::predecessor_account_id();
        let us = self.get_unstaked(&account_id);
        require!(
            us.as_yoctonear() >= amount.as_yoctonear(),
            "insufficient unstaked balance"
        );
        let new_u = us.checked_sub(amount).expect("unstaked underflow");
        if new_u.as_yoctonear() == 0 {
            self.unstaked.remove(&account_id);
        } else {
            self.unstaked.insert(account_id.clone(), new_u);
        }
        Promise::new(account_id).transfer(amount)
    }
}

impl MockStakingPoolContract {
    /// Zero when no entry (same as default balance on a real pool view).
    fn get_unstaked(&self, account_id: &AccountId) -> NearToken {
        self.unstaked
            .get(account_id)
            .copied()
            .unwrap_or_else(|| NearToken::from_near(0))
    }

    fn get_staked(&self, account_id: &AccountId) -> NearToken {
        self.staked
            .get(account_id)
            .copied()
            .unwrap_or_else(|| NearToken::from_near(0))
    }

    /// Removes the map entry when balance hits zero to keep storage tight in long test runs.
    fn insert_staked(&mut self, account_id: &AccountId, value: NearToken) {
        if value.as_yoctonear() == 0 {
            self.staked.remove(account_id);
        } else {
            self.staked.insert(account_id.clone(), value);
        }
    }
}
