use crate::*;
use near_sdk::{near, AccountId, NearToken, Promise, PublicKey};

#[near]
impl LockupContract {
    /// OWNER'S METHOD
    ///
    /// Requires 75 TGas (3 * BASE_GAS)
    ///
    /// Selects staking pool contract at the given account ID. The staking pool first has to be
    /// checked against the staking pool whitelist contract.
    pub fn select_staking_pool(&mut self, staking_pool_account_id: AccountId) -> Promise {
        self.assert_owner();
        assert!(
            env::is_valid_account_id(staking_pool_account_id.as_bytes()),
            "The staking pool account ID is invalid"
        );
        self.assert_staking_pool_is_not_selected();
        self.assert_no_termination();

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
    pub fn unselect_staking_pool(&mut self) {
        self.assert_owner();
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();
        // NOTE: This is best effort checks. There is still some balance might be left on the
        // staking pool, but it's up to the owner whether to unselect the staking pool.
        // The contract doesn't care about leftovers.
        assert_eq!(
            self.staking_information.as_ref().unwrap().deposit_amount.0,
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
    pub fn deposit_to_staking_pool(&mut self, amount: WrappedBalance) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();
        assert!(
            self.get_account_balance().0 >= amount.0,
            "The balance that can be deposited to the staking pool is lower than the extra amount"
        );

        env::log_str(&format!(
            "Depositing {} to the staking pool @{}",
            amount.0,
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
    pub fn deposit_and_stake(&mut self, amount: WrappedBalance) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();
        assert!(
            self.get_account_balance().0 >= amount.0,
            "The balance that can be deposited to the staking pool is lower than the extra amount"
        );

        env::log_str(&format!(
            "Depositing and staking {} to the staking pool @{}",
            amount.0,
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
    pub fn refresh_staking_pool_balance(&mut self) -> Promise {
        self.assert_owner();
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

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
    pub fn withdraw_from_staking_pool(&mut self, amount: WrappedBalance) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

        env::log_str(&format!(
            "Withdrawing {} from the staking pool @{}",
            amount.0,
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
    pub fn withdraw_all_from_staking_pool(&mut self) -> Promise {
        self.assert_owner();
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

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
    pub fn stake(&mut self, amount: WrappedBalance) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

        env::log_str(&format!(
            "Staking {} at the staking pool @{}",
            amount.0,
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
    pub fn unstake(&mut self, amount: WrappedBalance) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

        env::log_str(&format!(
            "Unstaking {} from the staking pool @{}",
            amount.0,
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
    pub fn unstake_all(&mut self) -> Promise {
        self.assert_owner();
        self.assert_staking_pool_is_idle();
        self.assert_no_termination();

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
    /// Requires 75 TGas (3 * BASE_GAS)
    /// Not intended to hand over the access to someone else except the owner
    ///
    /// Calls voting contract to validate if the transfers were enabled by voting. Once transfers
    /// are enabled, they can't be disabled anymore.
    pub fn check_transfers_vote(&mut self) -> Promise {
        // TODO: Deprecate.
        self.assert_owner();
        self.assert_transfers_disabled();
        self.assert_no_termination();

        let transfer_poll_account_id = match &self.lockup_information.transfers_information {
            TransfersInformation::TransfersDisabled {
                transfer_poll_account_id,
            } => transfer_poll_account_id,
            _ => unreachable!(),
        };

        env::log_str(&format!(
            "Checking that transfers are enabled at the transfer poll contract @{}",
            transfer_poll_account_id,
        ));

        ext_transfer_poll::ext(transfer_poll_account_id.clone())
            .with_static_gas(gas::transfer_poll::GET_RESULT)
            .get_result()
            .then(
                ext_self_owner::ext(env::current_account_id())
                    .with_static_gas(gas::owner_callbacks::ON_VOTING_GET_RESULT)
                    .on_get_result_from_transfer_poll(),
            )
    }

    /// OWNER'S METHOD
    ///
    /// Requires 50 TGas (2 * BASE_GAS)
    /// Not intended to hand over the access to someone else except the owner
    ///
    /// Transfers the given amount to the given receiver account ID.
    /// This requires transfers to be enabled within the voting contract.
    pub fn transfer(&mut self, amount: WrappedBalance, receiver_id: AccountId) -> Promise {
        self.assert_owner();
        assert!(amount.0 > 0, "Amount should be positive");
        assert!(
            env::is_valid_account_id(receiver_id.as_bytes()),
            "The receiver account ID is invalid"
        );
        self.assert_transfers_enabled();
        self.assert_no_staking_or_idle();
        self.assert_no_termination();
        assert!(
            self.get_liquid_owners_balance().0 >= amount.0,
            "The available liquid balance {} is smaller than the requested transfer amount {}",
            self.get_liquid_owners_balance().0,
            amount.0,
        );

        env::log_str(&format!(
            "Transferring {} to account @{}",
            amount.0, receiver_id
        ));

        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount.0))
    }

    /// OWNER'S METHOD
    ///
    /// Requires 50 TGas (2 * BASE_GAS)
    /// Not intended to hand over the access to someone else except the owner
    ///
    /// Adds full access key with the given public key to the account.
    /// The following requirements should be met:
    /// - The contract is fully vested;
    /// - Lockup duration has expired;
    /// - Transfers are enabled;
    /// - If there’s a termination made by foundation, it has to be finished.
    /// Full access key will allow owner to use this account as a regular account and remove
    /// the contract.
    pub fn add_full_access_key(&mut self, new_public_key: PublicKey) -> Promise {
        self.assert_owner();
        self.assert_transfers_enabled();
        self.assert_no_staking_or_idle();
        self.assert_no_termination();
        assert_eq!(
            self.get_locked_amount().0,
            0,
            "Tokens are still locked/unvested"
        );

        env::log_str("Adding a full access key");

        let new_public_key: PublicKey = new_public_key.into();

        Promise::new(env::current_account_id()).add_full_access_key(new_public_key)
    }
}
