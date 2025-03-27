use crate::*;
use common::events;
use near_sdk::{assert_one_yocto, near, AccountId, NearToken, Promise};

#[near]
impl LockupContract {
    /// OWNER'S METHOD
    ///
    /// Requires 75 TGas (3 * BASE_GAS)
    ///
    /// Selects staking pool contract at the given account ID. The staking pool first has to be
    /// checked against the staking pool whitelist contract.
    #[payable]
    pub fn select_staking_pool(&mut self, staking_pool_account_id: AccountId) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(
            env::is_valid_account_id(staking_pool_account_id.as_bytes()),
            "The staking pool account ID is invalid"
        );
        self.assert_staking_pool_is_not_selected();

        env::log_str(&format!(
            "Selecting staking pool @{}. Going to check whitelist first.",
            staking_pool_account_id
        ));

        ext_whitelist::ext(self.staking_pool_whitelist_account_id.clone())
            .with_static_gas(gas::whitelist::IS_WHITELISTED)
            .is_whitelisted(staking_pool_account_id.clone())
            .then(
                ext_self_owner::ext(env::current_account_id())
                    .with_static_gas(gas::owner_callbacks::ON_WHITELIST_IS_WHITELISTED)
                    .on_whitelist_is_whitelisted(staking_pool_account_id),
            )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 25 TGas (1 * BASE_GAS)
    ///
    /// Unselects the current staking pool.
    /// It requires that there are no known deposits left on the currently selected staking pool.
    #[payable]
    pub fn unselect_staking_pool(&mut self) {
        self.assert_owner();
        assert_one_yocto();
        self.assert_staking_pool_is_idle();
        // NOTE: This is best effort checks. There is still some balance might be left on the
        // staking pool, but it's up to the owner whether to unselect the staking pool.
        // The contract doesn't care about leftovers.
        assert_eq!(
            self.staking_information
                .as_ref()
                .unwrap()
                .deposit_amount
                .as_yoctonear(),
            0,
            "There is still a deposit on the staking pool"
        );

        env::log_str(&format!(
            "Unselected current staking pool @{}.",
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.staking_information = None;
    }

    /// OWNER'S METHOD
    ///
    /// Requires 100 TGas (4 * BASE_GAS)
    ///
    /// Deposits the given extra amount to the staking pool
    #[payable]
    pub fn deposit_to_staking_pool(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        assert!(
            self.get_account_balance() >= amount,
            "The balance that can be deposited to the staking pool is lower than the extra amount"
        );

        env::log_str(&format!(
            "Depositing {} to the staking pool @{}",
            amount,
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::DEPOSIT)
        .with_attached_deposit(amount)
        .deposit()
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_DEPOSIT)
                .on_staking_pool_deposit(amount),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 125 TGas (5 * BASE_GAS)
    ///
    /// Deposits and stakes the given extra amount to the selected staking pool
    #[payable]
    pub fn deposit_and_stake(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        assert!(
            self.get_account_balance() >= amount,
            "The balance that can be deposited to the staking pool is lower than the extra amount"
        );

        env::log_str(&format!(
            "Depositing and staking {} to the staking pool @{}",
            amount,
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::DEPOSIT_AND_STAKE)
        .with_attached_deposit(amount)
        .deposit_and_stake()
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_DEPOSIT_AND_STAKE)
                .on_staking_pool_deposit_and_stake(amount),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 75 TGas (3 * BASE_GAS)
    ///
    /// Retrieves total balance from the staking pool and remembers it internally.
    /// This method is helpful when the owner received some rewards for staking and wants to
    /// transfer them back to this account for withdrawal. In order to know the actual liquid
    /// balance on the account, this contract needs to query the staking pool.
    #[payable]
    pub fn refresh_staking_pool_balance(&mut self) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Fetching total balance from the staking pool @{}",
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::GET_ACCOUNT_TOTAL_BALANCE)
        .get_account_total_balance(env::current_account_id())
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_GET_ACCOUNT_TOTAL_BALANCE)
                .on_get_account_total_balance(),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 125 TGas (5 * BASE_GAS)
    ///
    /// Withdraws the given amount from the staking pool
    #[payable]
    pub fn withdraw_from_staking_pool(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Withdrawing {} from the staking pool @{}",
            amount,
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::WITHDRAW)
        .withdraw(amount)
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_WITHDRAW)
                .on_staking_pool_withdraw(amount),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 175 TGas (7 * BASE_GAS)
    ///
    /// Tries to withdraws all unstaked balance from the staking pool
    #[payable]
    pub fn withdraw_all_from_staking_pool(&mut self) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Going to query the unstaked balance at the staking pool @{}",
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::GET_ACCOUNT_UNSTAKED_BALANCE)
        .get_account_unstaked_balance(env::current_account_id())
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(
                    gas::owner_callbacks::ON_GET_ACCOUNT_UNSTAKED_BALANCE_TO_WITHDRAW_BY_OWNER,
                )
                .on_get_account_unstaked_balance_to_withdraw_by_owner(),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 125 TGas (5 * BASE_GAS)
    ///
    /// Stakes the given extra amount at the staking pool
    #[payable]
    pub fn stake(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Staking {} at the staking pool @{}",
            amount,
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::STAKE)
        .stake(amount)
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_STAKE)
                .on_staking_pool_stake(amount),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 125 TGas (5 * BASE_GAS)
    ///
    /// Unstakes the given amount at the staking pool
    #[payable]
    pub fn unstake(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Unstaking {} from the staking pool @{}",
            amount,
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        ext_staking_pool::ext(
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
                .clone(),
        )
        .with_static_gas(gas::staking_pool::UNSTAKE)
        .unstake(amount)
        .then(
            ext_self_owner::ext(env::current_account_id())
                .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_UNSTAKE)
                .on_staking_pool_unstake(amount),
        )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 125 TGas (5 * BASE_GAS)
    ///
    /// Unstakes all tokens from the staking pool
    #[payable]
    pub fn unstake_all(&mut self) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        self.assert_staking_pool_is_idle();

        env::log_str(&format!(
            "Unstaking all tokens from the staking pool @{}",
            self.staking_information
                .as_ref()
                .unwrap()
                .staking_pool_account_id
        ));

        self.set_staking_pool_status(TransactionStatus::Busy);

        let staking_pool_account_id: AccountId = self
            .staking_information
            .as_ref()
            .unwrap()
            .staking_pool_account_id
            .clone();

        ext_staking_pool::ext(staking_pool_account_id)
            .with_static_gas(gas::staking_pool::UNSTAKE_ALL)
            .unstake_all()
            .then(
                ext_self_owner::ext(env::current_account_id())
                    .with_static_gas(gas::owner_callbacks::ON_STAKING_POOL_UNSTAKE_ALL)
                    .on_staking_pool_unstake_all(),
            )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 50 TGas (2 * BASE_GAS)
    /// Not intended to hand over the access to someone else except the owner
    ///
    /// Transfers the given amount to the given receiver account ID.
    /// This requires transfers to be enabled within the voting contract.
    #[payable]
    pub fn transfer(&mut self, amount: NearToken, receiver_id: AccountId) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        assert!(amount.as_yoctonear() > 0, "Amount should be positive");
        assert!(
            env::is_valid_account_id(receiver_id.as_bytes()),
            "The receiver account ID is invalid"
        );
        self.assert_no_staking_or_idle();
        assert!(
            self.get_liquid_owners_balance() >= amount,
            "The available liquid balance {} is smaller than the requested transfer amount {}",
            self.get_liquid_owners_balance().as_yoctonear(),
            amount.as_yoctonear(),
        );

        assert!(
            NearToken::from_yoctonear(self.venear_liquid_balance()) >= amount,
            "The available liquid balance {} is smaller than the requested transfer amount {}",
            self.venear_liquid_balance(),
            amount
        );

        env::log_str(&format!(
            "Transferring {} to account @{}",
            amount, receiver_id
        ));

        Promise::new(receiver_id).transfer(amount)
    }

    /// OWNER'S METHOD
    /// Removes the lockup contract and transfers all NEAR to the initial owner.
    #[payable]
    pub fn delete_lockup(&mut self) -> Promise {
        self.assert_owner();
        assert_one_yocto();
        self.assert_no_staking_or_idle();
        assert_eq!(
            self.get_known_deposited_balance().as_yoctonear(),
            0,
            "Can't delete account with non-zero staked NEAR balance"
        );

        assert_eq!(
            self.venear_locked_balance, 0,
            "Can't delete account with non-zero locked venear balance"
        );
        assert_eq!(
            self.venear_pending_balance, 0,
            "Can't delete account with non-zero pending venear balance"
        );

        events::emit::lockup_action(
            "lockup_delete",
            &env::predecessor_account_id(),
            self.version,
            &Some(U64::from(self.lockup_update_nonce)),
            &None,
            &None,
        );

        Promise::new(env::current_account_id()).delete_account(self.owner_account_id.clone())
    }
}
