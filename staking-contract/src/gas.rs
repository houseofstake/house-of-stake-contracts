use crate::*;
use near_sdk::{Gas, env, require};

pub const BASE_GAS: Gas = Gas::from_gas(10_000_000_000_000);
pub const EPOCH_SETTLEMENT_MIN_GAS: Gas = Gas::from_gas(BASE_GAS.as_gas() * 30);

pub mod staking_pool {
    use near_sdk::Gas;
    pub const DEPOSIT_AND_STAKE: Gas = Gas::from_gas(super::BASE_GAS.as_gas() * 3);
    pub const UNSTAKE: Gas = Gas::from_gas(super::BASE_GAS.as_gas() * 3);
    /// Pull unstaked NEAR from pool into this contract.
    pub const WITHDRAW: Gas = Gas::from_gas(super::BASE_GAS.as_gas() * 3);
    /// Pool `get_account` (staked + unstaked + `can_withdraw` in one view).
    pub const GET_ACCOUNT: Gas = super::BASE_GAS;
    /// View call to the pool's `get_owner_id`.
    pub const GET_OWNER_ID: Gas = super::BASE_GAS;
}

pub mod callbacks {
    use super::BASE_GAS;
    use near_sdk::Gas;

    // Epoch settlement invocation chain (A -> B) and attached gas budget.
    //
    // Chain overview:
    // 0) promise_validator_per_epoch_settlement_then
    //    -> ON_EPOCH_SETTLEMENT_AFTER_POOL_ACCOUNT (26 * BASE)
    //       -> (optional) ON_GET_UNSTAKED_FOR_WITHDRAW (19 * BASE)
    //          -> ON_WITHDRAW_TRANSFER (2 * BASE)
    //          -> ON_EPOCH_SETTLEMENT_DISPATCH (12 * BASE)
    //                -> one of Pipeline-5 tails (8 * BASE):
    //                   ON_LOCK_FINALLY_MINT | ON_UNLOCK_TAIL_AFTER_PRE_USER
    //                   | ON_WITHDRAW_TAIL_AFTER_PRE_USER | ON_SUBSCRIPTION_UPDATE_AFTER_SETTLE
    //                   | ON_FARM_STAKE_AFTER_SETTLE | ON_FARM_UNSTAKE_AFTER_SETTLE
    //                -> release callback:
    //                   ON_EPOCH_PIPELINE_TERMINAL_RELEASE (1 * BASE)
    //                   or ON_EPOCH_PIPELINE_RELEASE_WITH_LOCK_ID (2 * BASE)
    //                   or ON_EPOCH_PIPELINE_RELEASE_WITH_SUBSCRIPTION_UPDATE_OUTCOME (2 * BASE)
    //                   or ON_EPOCH_PIPELINE_RELEASE_WITH_FARM_POSITION (2 * BASE)
    //
    // Fast path:
    // 0) promise_validator_per_epoch_settlement_then
    //    -> ON_EPOCH_SETTLEMENT_DISPATCH (12 * BASE)
    //       -> Pipeline-5 tail (8 * BASE) -> Pipeline-6 release (up to 2 * BASE)
    //
    // Gas budgeting rule used here:
    // If function A attaches gas for function B, budget A >= (gas attached to B) + A's own execution overhead.
    // All constants below follow this rule with a safety margin where callbacks fan out or chain release.
    pub const ON_DEPOSIT_AND_STAKE: Gas = BASE_GAS;
    pub const ON_UNSTAKE: Gas = BASE_GAS;
    /// After pool `withdraw`: continue into settlement `try_epoch_stake_or_unstake` + dispatch.
    ///
    /// Worst case in this callback is routing through `try_epoch_stake_or_unstake`, which may attach:
    /// `3 * BASE (pool settle)` + `1 * BASE (pool callback)` + `12 * BASE (dispatch)` + callback overhead.
    pub const ON_GET_UNSTAKED_FOR_WITHDRAW: Gas = Gas::from_gas(BASE_GAS.as_gas() * 19);
    pub const ON_WITHDRAW_TRANSFER: Gas = Gas::from_gas(BASE_GAS.as_gas() * 2);
    pub const ON_TOTAL_BALANCE: Gas = BASE_GAS;
    /// Balance refresh then catalog mint, usage bumps, subscription index, and `try_epoch_stake_or_unstake`.
    pub const ON_LOCK_REFRESH_THEN_FINALIZE: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// After `get_owner_id`: callback does a few storage writes (catalog or owner cache refresh).
    pub const ON_VALIDATOR_OWNER_CHECK: Gas = Gas::from_gas(BASE_GAS.as_gas() * 2);
    /// Tail dispatch after shared per-epoch settlement (`UserAction`).
    ///
    /// This callback fans out into Pipeline 5 tails (currently `8 * BASE_GAS`) and then chains
    /// Pipeline 6 release (up to `2 * BASE_GAS`), plus its own execution overhead.
    ///
    /// Budget formula: `8 * BASE (tail) + 2 * BASE (release) + 2 * BASE (dispatch overhead)`.
    pub const ON_EPOCH_SETTLEMENT_DISPATCH: Gas = Gas::from_gas(BASE_GAS.as_gas() * 12);
    /// After pool `get_account` during shared settlement (balance refresh + withdraw-if-ready).
    ///
    /// Worst case branch here schedules:
    /// `3 * BASE (pool withdraw)` + `2 * BASE (withdraw transfer callback)` +
    /// `19 * BASE (post-withdraw settle callback)` + callback overhead.
    pub const ON_EPOCH_SETTLEMENT_AFTER_POOL_ACCOUNT: Gas = Gas::from_gas(BASE_GAS.as_gas() * 26);
    /// Mint lock after shared pre-user settlement pipeline.
    pub const ON_LOCK_FINALLY_MINT: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// Unlock tail after pre-user settlement.
    pub const ON_UNLOCK_TAIL_AFTER_PRE_USER: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// Withdraw tail after pre-user settlement.
    pub const ON_WITHDRAW_TAIL_AFTER_PRE_USER: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// Subscription update after pre-user settlement.
    pub const ON_SUBSCRIPTION_UPDATE_AFTER_SETTLE: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// Farm stake after pre-user settlement.
    pub const ON_FARM_STAKE_AFTER_SETTLE: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// Farm unstake after pre-user settlement.
    pub const ON_FARM_UNSTAKE_AFTER_SETTLE: Gas = Gas::from_gas(BASE_GAS.as_gas() * 8);
    /// After user-flow tail promise completes: release pipeline `Busy`.
    pub const ON_EPOCH_PIPELINE_TERMINAL_RELEASE: Gas = BASE_GAS;
    /// Release pipeline `Busy`, return lock id, or refund payable entry on tail failure (transfer).
    pub const ON_EPOCH_PIPELINE_RELEASE_WITH_LOCK_ID: Gas = Gas::from_gas(BASE_GAS.as_gas() * 2);
    /// Release pipeline `Busy`, return subscription update outcome, or refund payable entry on tail failure.
    pub const ON_EPOCH_PIPELINE_RELEASE_WITH_SUBSCRIPTION_UPDATE_OUTCOME: Gas =
        Gas::from_gas(BASE_GAS.as_gas() * 2);
    /// Release pipeline `Busy`, return farm position, or refund payable entry on tail failure.
    pub const ON_EPOCH_PIPELINE_RELEASE_WITH_FARM_POSITION: Gas =
        Gas::from_gas(BASE_GAS.as_gas() * 2);
}

impl Contract {
    pub(crate) fn require_enough_gas_for_epoch_settlement(&self) {
        require!(
            env::prepaid_gas() >= EPOCH_SETTLEMENT_MIN_GAS,
            "Insufficient gas"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::callbacks;

    #[test]
    fn epoch_dispatch_budget_leaves_margin_after_value_returning_tail_and_release() {
        let max_value_returning_tail = [
            callbacks::ON_LOCK_FINALLY_MINT,
            callbacks::ON_SUBSCRIPTION_UPDATE_AFTER_SETTLE,
            callbacks::ON_FARM_STAKE_AFTER_SETTLE,
        ]
        .into_iter()
        .max_by_key(|gas| gas.as_gas())
        .expect("at least one value-returning tail budget");
        let max_value_returning_release = [
            callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_LOCK_ID,
            callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_SUBSCRIPTION_UPDATE_OUTCOME,
            callbacks::ON_EPOCH_PIPELINE_RELEASE_WITH_FARM_POSITION,
        ]
        .into_iter()
        .max_by_key(|gas| gas.as_gas())
        .expect("at least one value-returning release budget");

        let scheduled_child_gas =
            max_value_returning_tail.as_gas() + max_value_returning_release.as_gas();
        assert!(
            callbacks::ON_EPOCH_SETTLEMENT_DISPATCH.as_gas() > scheduled_child_gas,
            "dispatch callback needs explicit execution margin after scheduling value-returning tail and release"
        );
    }
}
