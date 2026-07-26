use crate::utils::{block_timestamp, near_from_shares};
use crate::*;
use common::U256;
use near_sdk::json_types::{U64, U128};
use near_sdk::{
    AccountId, NearToken, Promise, PromiseOrValue, assert_one_yocto, env, near, require,
};

pub const FARM_REWARD_RATE_DENOM: u128 = 1;
pub const FARM_ACC_REWARD_PER_SHARE_DENOM: u128 = 1_000_000_000_000_000_000_000_000;
pub const YOCTO_PER_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
pub const NS_PER_SECOND: u128 = 1_000_000_000;

#[near]
impl Contract {
    #[payable]
    pub fn stake(
        &mut self,
        product_id: ProductId,
        price_id: Option<PriceId>,
    ) -> PromiseOrValue<FarmPosition> {
        self.require_enough_gas_for_epoch_settlement();
        self.assert_not_paused();
        let account_id = env::predecessor_account_id();
        self.ensure_min_base_storage(&account_id);
        let deposit = env::attached_deposit();
        require!(
            deposit.as_yoctonear() >= self.internal_get_config().min_lock_amount.as_yoctonear(),
            "Attached NEAR is below the contract minimum lock amount (min_lock_amount)"
        );

        let price_id = self.resolve_farm_price_id(&product_id, price_id);
        let (price, product) = self.get_active_price_and_product(&price_id);
        self.require_farm_price_for_product(&price, &product_id);
        self.assert_validator_active_for_lock(&product.validator_id);
        require!(
            deposit.as_yoctonear() >= price.amount.0,
            "Attached NEAR is below the farm price minimum amount"
        );
        self.validate_farm_max_amount(&account_id, &product_id, &price, deposit.as_yoctonear());

        if let Some(position) = self.internal_get_farm_position(&account_id, &product_id) {
            if position.status == FarmStatus::Active {
                require!(
                    position.price_id == price_id,
                    "Existing farm position uses a different price"
                );
            }
        } else {
            self.ensure_min_storage_for_new_farm_position(&account_id);
        }

        let validator_id = product.validator_id.clone();
        let _validator = self.require_validator_idle(&validator_id);

        #[cfg(not(target_arch = "wasm32"))]
        {
            return PromiseOrValue::Value(self.commit_farm_stake(
                account_id,
                deposit,
                product_id,
                price_id,
                validator_id,
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            return self
                .promise_validator_per_epoch_settlement_then(
                    validator_id.clone(),
                    UserAction::CommitFarmStake {
                        validator_id,
                        account_id,
                        deposit,
                        product_id,
                        price_id,
                    },
                )
                .into();
        }
    }

    #[payable]
    pub fn unstake(&mut self, product_id: ProductId, amount: Option<U128>) -> Promise {
        self.require_enough_gas_for_epoch_settlement();
        assert_one_yocto();
        self.assert_not_paused();

        let account_id = env::predecessor_account_id();
        self.ensure_min_base_storage(&account_id);
        let position = self.require_active_farm_position(&account_id, &product_id);
        let _validator = self.require_validator_idle(&position.validator_id);

        self.promise_validator_per_epoch_settlement_then(
            position.validator_id.clone(),
            UserAction::FarmUnstakeQueue {
                validator_id: position.validator_id,
                account_id,
                product_id,
                amount,
            },
        )
    }

    pub fn get_farm_pool(&self, price_id: PriceId) -> Option<FarmPool> {
        self.internal_get_farm_pool(&price_id)
    }

    pub fn get_farm_position(
        &self,
        account_id: AccountId,
        product_id: ProductId,
    ) -> Option<FarmPositionView> {
        self.internal_get_farm_position(&account_id, &product_id)
            .map(|position| self.farm_position_view(position))
    }

    pub fn get_farm_positions_for_account(
        &self,
        account_id: AccountId,
        from_index: u64,
        limit: u64,
    ) -> Vec<FarmPositionView> {
        let product_ids = self
            .farm_position_products_by_account
            .get(&account_id)
            .cloned()
            .unwrap_or_default();
        self.collect_paginated(from_index, limit, product_ids.len() as u64, |index| {
            product_ids
                .get(index as usize)
                .and_then(|product_id| self.internal_get_farm_position(&account_id, product_id))
                .map(|position| self.farm_position_view(position))
        })
    }

    pub fn get_farm_account(&self, account_id: AccountId) -> FarmAccountView {
        let account = self
            .internal_get_farm_account(&account_id)
            .unwrap_or(FarmAccount {
                account_id: account_id.clone(),
                accumulated_reward_units: U128(0),
                active_position_count: 0,
                last_update_ns: U64(0),
            });
        let mut unclaimed = 0u128;
        let mut active_positions = Vec::new();
        for position in self.get_farm_positions_for_account(account_id.clone(), 0, u64::MAX) {
            if position.status != FarmStatus::Active || position.shares.0 == 0 {
                continue;
            }
            unclaimed = unclaimed.saturating_add(position.pending_reward_units.0);
            active_positions.push(position);
        }
        FarmAccountView {
            account_id,
            accumulated_reward_units: account.accumulated_reward_units,
            pending_reward_units: U128(unclaimed),
            total_earned_reward_units: U128(
                account.accumulated_reward_units.0.saturating_add(unclaimed),
            ),
            active_positions,
        }
    }
}

#[near]
impl Contract {
    #[private]
    pub fn resolve_farm_stake(
        &mut self,
        account_id: AccountId,
        deposit: NearToken,
        product_id: ProductId,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> FarmPosition {
        let _validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );
        self.commit_farm_stake(account_id, deposit, product_id, price_id, validator_id)
    }

    #[private]
    pub fn resolve_farm_unstake(
        &mut self,
        account_id: AccountId,
        product_id: ProductId,
        validator_id: ValidatorId,
        amount: Option<U128>,
    ) {
        let validator = self.require_validator_busy(
            &validator_id,
            "Validator pool must be busy after per-epoch settlement",
        );
        let position = self.require_active_farm_position(&account_id, &product_id);
        let shares_remove = self.farm_unstake_shares_for_amount(&position, &validator, amount);
        self.commit_farm_unstake(account_id, product_id, validator_id, shares_remove);
    }
}

impl Contract {
    pub(crate) fn internal_get_farm_pool(&self, id: &PriceId) -> Option<FarmPool> {
        self.farm_pools.get(id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_farm_pool(&mut self, id: PriceId, pool: FarmPool) {
        self.farm_pools.insert(id, pool.into());
    }

    pub(crate) fn require_farm_pool(&self, price_id: &PriceId) -> FarmPool {
        self.internal_get_farm_pool(price_id)
            .unwrap_or_else(|| env::panic_str("Farm pool not found"))
    }

    pub(crate) fn internal_get_farm_position(
        &self,
        account_id: &AccountId,
        product_id: &ProductId,
    ) -> Option<FarmPosition> {
        self.farm_positions
            .get(&(account_id.clone(), product_id.clone()))
            .cloned()
            .map(Into::into)
    }

    pub(crate) fn internal_set_farm_position(&mut self, position: FarmPosition) {
        self.farm_positions.insert(
            (position.account_id.clone(), position.product_id.clone()),
            position.into(),
        );
    }

    pub(crate) fn internal_get_farm_account(&self, account_id: &AccountId) -> Option<FarmAccount> {
        self.farm_accounts.get(account_id).cloned().map(Into::into)
    }

    pub(crate) fn internal_set_farm_account(
        &mut self,
        account_id: AccountId,
        account: FarmAccount,
    ) {
        self.farm_accounts.insert(account_id, account.into());
    }

    pub(crate) fn require_no_active_farm_price_for_product(&self, product: &Product) {
        for price_id in &product.price_ids {
            if let Some(price) = self.internal_get_price(price_id) {
                require!(
                    price.price_type != PriceType::Farm || price.status != CatalogStatus::Active,
                    "Product already has an active farm price"
                );
            }
        }
    }

    pub(crate) fn settle_farm_pool(&mut self, price_id: &PriceId) {
        let pool = self.require_farm_pool(price_id);
        let settled = self.simulate_settled_farm_pool(pool);
        self.internal_set_farm_pool(price_id.clone(), settled);
    }

    fn simulate_settled_farm_pool(&self, mut pool: FarmPool) -> FarmPool {
        let now_ns = block_timestamp();
        if now_ns <= pool.last_reward_settle_ns.0 {
            return pool;
        }
        if pool.total_farm_shares.0 == 0 {
            pool.last_reward_settle_ns = U64(now_ns);
            return pool;
        }

        let product = self.require_product(&pool.product_id);
        let validator = self.require_validator(&product.validator_id);
        let total_farm_near_yocto = near_from_shares(
            pool.total_farm_shares.0,
            validator.net_stake_yocto(),
            validator.total_shares.0,
        );
        let elapsed_ns = now_ns.saturating_sub(pool.last_reward_settle_ns.0);
        let delta_reward_units = farm_delta_reward_units(
            total_farm_near_yocto,
            u128::from(elapsed_ns),
            pool.reward_rate.0,
        );
        if delta_reward_units > 0 {
            let delta_acc = ((U256::from(delta_reward_units)
                * U256::from(FARM_ACC_REWARD_PER_SHARE_DENOM))
                / U256::from(pool.total_farm_shares.0))
            .as_u128();
            pool.acc_reward_per_share = U128(pool.acc_reward_per_share.0.saturating_add(delta_acc));
        }
        pool.last_reward_settle_ns = U64(now_ns);
        pool
    }

    fn settle_farm_position(&mut self, account_id: &AccountId, product_id: &ProductId) -> u128 {
        let mut position = self.require_active_farm_position(account_id, product_id);
        self.settle_farm_pool(&position.price_id);
        let pool = self.require_farm_pool(&position.price_id);
        let accumulated = farm_position_accumulated(position.shares.0, pool.acc_reward_per_share.0);
        let pending = accumulated.saturating_sub(position.reward_debt.0);
        position.accrued_reward_units =
            U128(position.accrued_reward_units.0.saturating_add(pending));
        position.reward_debt = U128(accumulated);
        position.updated_ns = U64(block_timestamp());
        self.internal_set_farm_position(position);
        pending
    }

    fn preview_farm_position_pending(&self, position: &FarmPosition) -> u128 {
        let pool = self.simulate_settled_farm_pool(self.require_farm_pool(&position.price_id));
        let accumulated = farm_position_accumulated(position.shares.0, pool.acc_reward_per_share.0);
        position
            .accrued_reward_units
            .0
            .saturating_add(accumulated.saturating_sub(position.reward_debt.0))
    }

    fn farm_position_view(&self, position: FarmPosition) -> FarmPositionView {
        let staked_near_amount = if position.status == FarmStatus::Active && position.shares.0 > 0 {
            let validator = self.require_validator(&position.validator_id);
            near_from_shares(
                position.shares.0,
                validator.net_stake_yocto(),
                validator.total_shares.0,
            )
        } else {
            0
        };
        let pending_reward_units = if position.status == FarmStatus::Active && position.shares.0 > 0
        {
            self.preview_farm_position_pending(&position)
        } else {
            position.accrued_reward_units.0
        };
        FarmPositionView {
            account_id: position.account_id,
            product_id: position.product_id,
            price_id: position.price_id,
            validator_id: position.validator_id,
            shares: position.shares,
            staked_near_amount: U128(staked_near_amount),
            reward_debt: position.reward_debt,
            accrued_reward_units: position.accrued_reward_units,
            pending_reward_units: U128(pending_reward_units),
            total_earned_reward_units: U128(pending_reward_units),
            status: position.status,
            created_ns: position.created_ns,
            updated_ns: position.updated_ns,
        }
    }

    fn commit_farm_stake(
        &mut self,
        account_id: AccountId,
        deposit: NearToken,
        product_id: ProductId,
        price_id: PriceId,
        validator_id: ValidatorId,
    ) -> FarmPosition {
        let (mut price, mut product) = self.get_active_price_and_product(&price_id);
        self.require_farm_price_for_product(&price, &product_id);
        require!(
            product.validator_id == validator_id,
            "Catalog validator for this farm price does not match the pool used for this stake"
        );
        self.validate_farm_max_amount(&account_id, &product_id, &price, deposit.as_yoctonear());

        let is_new_position = self
            .internal_get_farm_position(&account_id, &product_id)
            .is_none();
        if is_new_position {
            self.ensure_min_storage_for_new_farm_position(&account_id);
        }

        if let Some(existing) = self.internal_get_farm_position(&account_id, &product_id) {
            if existing.status == FarmStatus::Active {
                let _ = self.settle_farm_position(&account_id, &product_id);
            } else {
                self.settle_farm_pool(&price_id);
            }
        } else {
            self.settle_farm_pool(&price_id);
        }

        let added_shares = self.internal_stake(&account_id, &validator_id, deposit);
        let mut pool = self.require_farm_pool(&price_id);
        pool.total_farm_shares = U128(pool.total_farm_shares.0.saturating_add(added_shares));
        self.internal_set_farm_pool(price_id.clone(), pool.clone());

        let now = block_timestamp();
        let mut position = self
            .internal_get_farm_position(&account_id, &product_id)
            .unwrap_or(FarmPosition {
                account_id: account_id.clone(),
                product_id: product_id.clone(),
                price_id: price_id.clone(),
                validator_id: validator_id.clone(),
                shares: U128(0),
                reward_debt: U128(0),
                accrued_reward_units: U128(0),
                status: FarmStatus::Active,
                created_ns: U64(now),
                updated_ns: U64(now),
            });
        let was_inactive = position.status != FarmStatus::Active || position.shares.0 == 0;
        if position.status == FarmStatus::Closed {
            position.price_id = price_id.clone();
            position.validator_id = validator_id.clone();
            position.shares = U128(0);
            position.reward_debt = U128(0);
            position.accrued_reward_units = U128(0);
            position.status = FarmStatus::Active;
            position.created_ns = U64(now);
        }
        position.shares = U128(position.shares.0.saturating_add(added_shares));
        position.reward_debt = U128(farm_position_accumulated(
            position.shares.0,
            pool.acc_reward_per_share.0,
        ));
        position.updated_ns = U64(now);
        self.internal_set_farm_position(position.clone());
        if was_inactive {
            self.increment_active_farm_position_count(&account_id);
        }
        if is_new_position {
            self.increment_farm_position_storage_count(&account_id);
        }

        self.add_farm_position_product_to_account(&account_id, &product_id);
        price.usage_count = price.usage_count.saturating_add(1);
        product.usage_count = product.usage_count.saturating_add(1);
        self.internal_set_price(price_id.clone(), price);
        self.internal_set_product(product_id.clone(), product);
        crate::events::log_farm_stake(
            &account_id,
            &product_id,
            &price_id,
            &validator_id,
            deposit.as_yoctonear(),
            added_shares,
        );
        position
    }

    fn commit_farm_unstake(
        &mut self,
        account_id: AccountId,
        product_id: ProductId,
        validator_id: ValidatorId,
        shares_remove: u128,
    ) {
        let _ = self.settle_farm_position(&account_id, &product_id);
        let mut position = self.require_active_farm_position(&account_id, &product_id);
        require!(
            position.validator_id == validator_id,
            "Farm validator mismatch"
        );
        require!(
            position.shares.0 >= shares_remove,
            "Cannot unstake more farm shares than the position holds"
        );

        let near_yocto =
            self.internal_unstake(account_id.clone(), validator_id.clone(), shares_remove);
        let mut pool = self.require_farm_pool(&position.price_id);
        require!(
            pool.total_farm_shares.0 >= shares_remove,
            "Farm pool shares underflow"
        );
        pool.total_farm_shares = U128(pool.total_farm_shares.0 - shares_remove);
        self.internal_set_farm_pool(position.price_id.clone(), pool.clone());

        position.shares = U128(position.shares.0 - shares_remove);
        position.reward_debt = U128(farm_position_accumulated(
            position.shares.0,
            pool.acc_reward_per_share.0,
        ));
        position.updated_ns = U64(block_timestamp());
        if position.shares.0 == 0 {
            let accrued = position.accrued_reward_units.0;
            let mut account = self
                .internal_get_farm_account(&account_id)
                .unwrap_or(FarmAccount {
                    account_id: account_id.clone(),
                    accumulated_reward_units: U128(0),
                    active_position_count: 0,
                    last_update_ns: U64(0),
                });
            account.accumulated_reward_units =
                U128(account.accumulated_reward_units.0.saturating_add(accrued));
            account.active_position_count = account.active_position_count.saturating_sub(1);
            account.last_update_ns = U64(block_timestamp());
            self.internal_set_farm_account(account_id.clone(), account);
            position.accrued_reward_units = U128(0);
            position.status = FarmStatus::Closed;
        }
        let price_id = position.price_id.clone();
        let remaining_shares = position.shares.0;
        self.internal_set_farm_position(position);
        crate::events::log_farm_unstake(
            &account_id,
            &product_id,
            &price_id,
            &validator_id,
            near_yocto,
            shares_remove,
            remaining_shares,
        );
    }

    fn resolve_farm_price_id(&self, product_id: &ProductId, price_id: Option<PriceId>) -> PriceId {
        if let Some(price_id) = price_id {
            let price = self.require_price(&price_id);
            self.require_farm_price_for_product(&price, product_id);
            require!(
                price.status == CatalogStatus::Active,
                "This farm price is archived or inactive"
            );
            return price_id;
        }
        let product = self.require_product(product_id);
        let mut found: Option<PriceId> = None;
        for candidate_id in &product.price_ids {
            if let Some(price) = self.internal_get_price(candidate_id) {
                if price.price_type == PriceType::Farm && price.status == CatalogStatus::Active {
                    require!(found.is_none(), "Product has multiple active farm prices");
                    found = Some(candidate_id.clone());
                }
            }
        }
        found.unwrap_or_else(|| env::panic_str("No active farm price for this product"))
    }

    fn require_farm_price_for_product(&self, price: &Price, product_id: &ProductId) {
        require!(
            price.price_type == PriceType::Farm,
            "This price is not a farm price"
        );
        require!(
            price.product_id == *product_id,
            "Farm price does not belong to this product"
        );
    }

    fn require_active_farm_position(
        &self,
        account_id: &AccountId,
        product_id: &ProductId,
    ) -> FarmPosition {
        let position = self
            .internal_get_farm_position(account_id, product_id)
            .unwrap_or_else(|| env::panic_str("Farm position not found"));
        require!(
            position.status == FarmStatus::Active,
            "Farm position is not active"
        );
        require!(position.shares.0 > 0, "Farm position has no shares");
        position
    }

    fn validate_farm_max_amount(
        &self,
        account_id: &AccountId,
        product_id: &ProductId,
        price: &Price,
        deposit_yocto: u128,
    ) {
        let Some(max_amount) = price.metadata.as_ref().and_then(|m| m.max_amount) else {
            return;
        };
        let current = self
            .internal_get_farm_position(account_id, product_id)
            .filter(|p| p.status == FarmStatus::Active)
            .map(|p| {
                let validator = self.require_validator(&p.validator_id);
                near_from_shares(
                    p.shares.0,
                    validator.net_stake_yocto(),
                    validator.total_shares.0,
                )
            })
            .unwrap_or(0);
        require!(
            current.saturating_add(deposit_yocto) <= max_amount.0,
            "Farm stake exceeds max_amount"
        );
    }

    fn farm_unstake_shares_for_amount(
        &self,
        position: &FarmPosition,
        validator: &Validator,
        amount: Option<U128>,
    ) -> u128 {
        let Some(amount) = amount else {
            return position.shares.0;
        };
        require!(amount.0 > 0, "Unstake amount must be greater than zero");
        let net_stake = validator.net_stake_yocto();
        require!(
            net_stake > 0,
            "Cannot price farm unstake with no effective stake"
        );
        let position_near =
            near_from_shares(position.shares.0, net_stake, validator.total_shares.0);
        if amount.0 >= position_near {
            return position.shares.0;
        }
        farm_shares_for_amount_ceil(
            amount.0,
            validator.total_shares.0,
            net_stake,
            position.shares.0,
        )
    }

    fn add_farm_position_product_to_account(
        &mut self,
        account_id: &AccountId,
        product_id: &ProductId,
    ) {
        let mut product_ids = self
            .farm_position_products_by_account
            .get(account_id)
            .cloned()
            .unwrap_or_default();
        if !product_ids.contains(product_id) {
            product_ids.push(product_id.clone());
            self.farm_position_products_by_account
                .insert(account_id.clone(), product_ids);
        }
    }

    fn increment_active_farm_position_count(&mut self, account_id: &AccountId) {
        let now = block_timestamp();
        let mut account = self
            .internal_get_farm_account(account_id)
            .unwrap_or(FarmAccount {
                account_id: account_id.clone(),
                accumulated_reward_units: U128(0),
                active_position_count: 0,
                last_update_ns: U64(now),
            });
        account.active_position_count = account.active_position_count.saturating_add(1);
        account.last_update_ns = U64(now);
        self.internal_set_farm_account(account_id.clone(), account);
    }

    fn increment_farm_position_storage_count(&mut self, account_id: &AccountId) {
        let count = self
            .user_farm_position_count
            .get(account_id)
            .copied()
            .unwrap_or(0);
        self.user_farm_position_count
            .insert(account_id.clone(), count.saturating_add(1));
    }
}

pub fn farm_delta_reward_units(
    total_farm_near_yocto: u128,
    elapsed_ns: u128,
    reward_rate: u128,
) -> u128 {
    (((U256::from(total_farm_near_yocto) * U256::from(elapsed_ns) * U256::from(reward_rate))
        / U256::from(YOCTO_PER_NEAR))
        / U256::from(NS_PER_SECOND)
        / U256::from(FARM_REWARD_RATE_DENOM))
    .as_u128()
}

pub fn farm_position_accumulated(shares: u128, acc_reward_per_share: u128) -> u128 {
    ((U256::from(shares) * U256::from(acc_reward_per_share))
        / U256::from(FARM_ACC_REWARD_PER_SHARE_DENOM))
    .as_u128()
}

pub fn farm_shares_for_amount_ceil(
    amount_yocto: u128,
    validator_total_shares: u128,
    net_stake_yocto: u128,
    max_shares: u128,
) -> u128 {
    let numerator = U256::from(amount_yocto) * U256::from(validator_total_shares);
    let denominator = U256::from(net_stake_yocto);
    let mut shares = (numerator / denominator).as_u128();
    if !(numerator % denominator).is_zero() {
        shares = shares.saturating_add(1);
    }
    shares.clamp(1, max_shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_rate_example_matches_spec() {
        let reward = farm_delta_reward_units(
            100 * YOCTO_PER_NEAR,
            6 * 86_400 * NS_PER_SECOND,
            3_858_024_691_358_024,
        );
        assert_eq!(reward, 199_999_999_999_999_964_160_000);
    }

    #[test]
    fn shares_for_partial_unstake_round_up() {
        assert_eq!(farm_shares_for_amount_ceil(4, 10, 11, 10), 4);
    }
}
