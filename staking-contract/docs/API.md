# staking-contract — public API

Reference for **on-chain methods** exposed by `staking-contract` (Rust type names below match JSON **camelCase** field names from [`near-sdk`] serialization unless noted).

**Conventions**

- **`#[payable]` + “attach 1 yocto”**: method calls [`near_sdk::assert_one_yocto()`](https://docs.rs/near-sdk/latest/near_sdk/fn.assert_one_yocto.html); attach exactly **1 yoctoNEAR**.
- **Other `#[payable]`**: attach the stated NEAR (e.g. storage, lock stake).
- **Catalog auth**: mutating catalog methods assert the caller is either the staking pool’s **`get_owner_id()`** for that validator’s pool (cross-contract view) or a catalog manager configured by that pool owner.
- **Pause**: when `paused == true`, most mutating user paths revert (see individual methods).

---

## Initialization

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `new` | Deploy init | — | — | **`#[init]`** — constructs contract from `config` (see [`Config`](../src/config.rs)). |

---

## Views (read-only)

| Method | Parameters | Returns | Description |
|--------|------------|---------|-------------|
| `get_config` | — | `Config` | Full governance & economics config. |
| `get_version` | — | `string` | Crate package version string. |
| `is_paused` | — | `bool` | Global pause flag. |
| `get_account` | `account_id: AccountId` | `Account \| null` | NEP-style prepaid **`storage_deposit`** only. |
| `get_accounts` | `from_index: u64`, `limit: u64` | `AccountView[]` | Paginated registered accounts from the enumerable account index, including each row's account id. |
| `get_account_ids` | `from_index: u64`, `limit: u64` | `AccountId[]` | Paginated registered account ids from the enumerable account index. |
| `storage_balance_bounds` | — | `StorageBalanceBounds` | NEP-145 minimum registration balance; `max` is `null` because storage top-ups are unbounded. |
| `storage_balance_of` | `account_id: AccountId` | `StorageBalance \| null` | NEP-145 storage balance; returns `null` for unregistered accounts. |
| `get_validator` | `validator_id: AccountId` | `Validator \| null` | Validator row for one staking pool contract account. |
| `get_validators` | `from_index: u64`, `limit: u64` | `Validator[]` | Paginated allowlist (stable ordering); each row’s **`validator_id`** is that pool’s account id. |
| `get_product` | `product_id: string` | `Product \| null` | Catalog product (`prod_*`). |
| `get_price` | `price_id: string` | `Price \| null` | Catalog price (`price_*`). |
| `get_products` | `from_index: u64`, `limit: u64` | `Product[]` | Paginated catalog (stable creation order in contract index). |
| `get_product_default_price` | `product_id: string` | `string \| null` | Same as **`Product.default_price_id`** from **`get_product`** / **`get_products`** — default catalog **`price_id`** (see **`set_product_default_price`**); **`null`** if unset. |
| `get_lock` | `lock_id: string`, `effective?: bool` | `Lock \| null` | Lock record (`lock_*`). By default, or with `effective=false`, returns the raw stored lock. With `effective=true`, the current active subscription lock can include due pending subscription update projection for amount, shares, status, period, and price. Projected stake decreases are view-only and do not represent actual `pending_to_unstake` or user pending-unstake accounting until a later mutating renewal/update path applies them. Historical subscription locks and cancelled subscriptions return raw locks. |
| `get_subscription` | `subscription_id: string` | `Subscription \| null` | Subscription (`sub_*`). |
| `get_subscription_for_product` | `account_id`, `product_id` | `Subscription \| null` | Lookup by `(account, product)`. |
| `get_subscription_for_price` | `account_id`, `price_id` | `Subscription \| null` | Resolves product from price, then same as above. |
| `get_subscriptions` | `from_index: u64`, `limit: u64` | `Subscription[]` | Paginated subscriptions, with due pending updates projected in the returned views. |
| `get_subscriptions_for_account` | `account_id`, `from_index: u64`, `limit: u64` | `Subscription[]` | Paginated subscriptions owned by an account, with due pending updates projected in the returned views. |
| `get_subscriptions_for_product` | `product_id`, `from_index: u64`, `limit: u64` | `Subscription[]` | Paginated subscriptions currently indexed under a product, with due pending updates projected in the returned views. |
| `get_purchase` | `purchase_id: string` | `Purchase \| null` | Direct payment purchase record (`pay_*`). |
| `get_purchases` | `from_index: u64`, `limit: u64` | `Purchase[]` | Paginated direct payment purchases. |
| `get_purchases_for_account` | `account_id`, `from_index: u64`, `limit: u64` | `Purchase[]` | Paginated direct payment purchases by buyer account. |
| `get_purchases_for_product` | `product_id`, `from_index: u64`, `limit: u64` | `Purchase[]` | Paginated direct payment purchases by product. |
| `get_revenue_balance_for_validator` | `validator_id: AccountId` | `NearToken` | Current withdrawable direct-payment revenue for the validator. |
| `get_farm_pool` | `price_id: string` | `FarmPool \| null` | Stored farm accumulator for a Farm price. |
| `get_farm_position` | `account_id`, `product_id` | `FarmPositionView \| null` | Stored farm position plus view-time `staked_near_amount`, `pending_reward_units`, and `total_earned_reward_units` for one `(account, product)`. |
| `get_farm_positions_for_account` | `account_id`, `from_index: u64`, `limit: u64` | `FarmPositionView[]` | Paginated current and historical farm positions for an account, including view-time staked NEAR and reward fields. |
| `get_farm_account` | `account_id` | `FarmAccountView` | Stored closed-position reward roll-up plus simulated pending rewards for active positions. `active_positions` contains `FarmPositionView` rows. Reward units are 24-decimal fixed-point accounting units (`1e24 == 1 whole reward unit`). |

---

## Storage & balances (`accounts.rs`)

| Method | Access | Deposit | Description |
|--------|--------|---------|-------------|
| `storage_deposit` | Any | **Attach NEAR** | NEP-145 register/top-up: `account_id?: AccountId`, `registration_only?: bool`. With `registration_only=true`, only the amount needed to reach `min_storage_deposit` is retained and excess is refunded. Non-registration top-ups must satisfy retained storage for `min_storage_deposit + per_lock_storage_stake × user_lock_count + per_farm_position_storage_stake × user_farm_position_count + per_purchase_storage_stake × user_purchase_count`. |
| `storage_withdraw` | Account owner | **1 yocto** + optional `amount: NearToken` | NEP-145 withdraw from `available`; omitting `amount` withdraws all available storage. |
| `storage_unregister` | Account owner | **1 yocto** + optional `force: bool` | NEP-145 unregister/refund when the account has no retained per-lock, per-purchase, subscription, farm-position, or pending-unstake storage. `force=true` is rejected; without force the method returns `false` instead of deleting accounts that still own retained records. |
| `get_accounts` | Anyone | 0 | `from_index`, `limit`. Lists registered account rows from the enumerable account index with `account_id` and storage fields. During upgrade, discoverable accounts from purchases, subscriptions, and legacy pending-unstake account references are backfilled; storage-only pre-upgrade accounts remain readable by `get_account` and appear in `get_accounts` after their next storage mutation. |
| `get_account_ids` | Anyone | 0 | `from_index`, `limit`. Lists registered account ids from the enumerable account index. During upgrade, discoverable accounts from purchases, subscriptions, and legacy pending-unstake account references are backfilled; storage-only pre-upgrade accounts remain readable and appear after their next storage mutation. |

---

## Validator allowlist (`validators.rs`)

**Caller:** contract **`owner_account_id`** (from `config`), unless noted.

| Method | Deposit | Description |
|--------|---------|-------------|
| `add_validator` | **1 yocto** | Allowlist a **`validator_id`** (staking pool contract account). Fails if that validator row already exists. |
| `add_validator_catalog_manager` | **1 yocto** | Validator pool owner only. Grant product/price catalog management rights for that validator to **`catalog_manager_account_id`**. Duplicate grants are no-ops. |
| `remove_validator_catalog_manager` | **1 yocto** | Validator pool owner only. Revoke catalog management rights for **`catalog_manager_account_id`**. Removing an absent catalog manager is a no-op. |
| `pause_validator` | **1 yocto** | Set validator **`ValidatorStatus::Paused`** (blocks **new** locks for that pool). |
| `remove_validator` | **1 yocto** | Mark **`Removed`** when no shares / pending stake / unstake / withdraw buckets (see contract checks). |

---

## Catalog — products & prices (`products.rs`)

All mutation entrypoints attach **1 yocto**, require contract **not paused**, validator **allowlisted**, then **`Promise`** chain: pool **`get_owner_id()`** → **`#[private]`** callback verifies the predecessor of the **original** call is either `pool_owner` or one of that validator's **`catalog_manager_account_ids`**.

**Returns:** most return **`Promise`** (async completion of catalog write).

| Method | Description |
|--------|-------------|
| `create_product` | `validator_id`, `name`, `description` → creates `prod_*`. |
| `edit_product` | `product_id`, `name`, `description`. |
| `archive_product` | `product_id`. |
| `unarchive_product` | `product_id` — restore **`CatalogStatus::Active`** (must currently be archived). |
| `delete_product` | `product_id` (invariants: no attached prices in use — see contract). |
| `create_price` | `product_id`, `name`, `description`, `amount` (`U128` yocto), `price_type`, `billing_period`, `lock_factor_near_months`, `metadata`. `PriceType::Farm` requires no billing period, `lock_factor_near_months == 0`, `metadata.farm_reward_rate`, and creates a linked `FarmPool`. At most one active Farm price may exist per product. For recurring variable-stake and farm prices, `metadata.max_amount` is an optional inclusive upper bound and must be `>= amount`. |
| `edit_price` | `price_id`, optional `name`, optional `description`, optional `metadata`. Farm prices may update `metadata.farm_reward_rate`; the farm pool is settled with the old rate first, then the new rate applies prospectively. |
| `archive_price` | `price_id`. |
| `unarchive_price` | `price_id` — restore **`CatalogStatus::Active`** (must currently be archived). |
| `delete_price` | `price_id`. |
| `set_product_default_price` | `product_id`, **`price_id`: optional** — set or clear **`Product.default_price_id`**. **`price_id`** must refer to an **active** (unarchived) catalog price on that product; archived prices are rejected (**unarchive** first). Cleared when **`archive_product`**, **`archive_price`** (if that price was the default), or **`delete_price`**. |

---

## Locks (`lock.rs`)

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `lock` | Buyer / subscriber | **Attach NEAR** | **`PromiseOrValue<LockId>`** | **`price_id`**, **`product_id`**, **`duration_ns`** — provide exactly one of **`price_id`** or **`product_id`**. One-off prices require **`duration_ns`** (`U64`). Recurring monthly subscription prices require **`duration_ns: null`** and derive the duration from the billing period. Default price from **`Product.default_price_id`** when only **`product_id`** is set. **WASM:** shared per-epoch pipeline (**0–3**) then mint (**5a**); see [features/lazy-epoch-pipeline.md](features/lazy-epoch-pipeline.md). **Host tests:** synchronous mint (no promise chain). |

## Staking farm (`stake.rs`)

Farm rewards are non-transferable accounting units. `FarmPool.reward_rate` is 24-decimal reward units per second per staked NEAR, scaled by `FARM_REWARD_RATE_DENOM = 1`. Accumulators use `FARM_ACC_REWARD_PER_SHARE_DENOM = 1_000_000_000_000_000_000_000_000`.

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `stake` | Registered account | **Attach NEAR** | **`PromiseOrValue<FarmPosition>`** | `product_id`, optional `price_id`. Resolves the active Farm price, validates amount/min/max, runs the shared validator settlement pipeline, mints validator shares, updates `FarmPool.total_farm_shares`, and creates or aggregates the account's one live farm position for the product. |
| `unstake` | Position owner | **1 yocto** | **`Promise`** | `product_id`, optional `amount: U128`. Burns all shares when `amount` is `null`; otherwise converts requested NEAR to shares with ceiling division after the validator settlement preamble. TODO: confirm that `amount: null` should remain the final full-exit API. Queues the same `user_pending_unstake` tranches as `unlock`; users later call `withdraw(validator_id)`. Full unstake closes the position and rolls accrued reward units into `FarmAccount`. |

## Direct payments (`payments.rs`)

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `pay` | Buyer | **Attach exact NEAR price × quantity** | `PurchaseId` | Direct one-off payment for `price_id` or a product default price. Requires prepaid storage for one additional purchase, an active one-off price with no billing period, and an active validator. Creates a `pay_*` purchase record, increments product/price usage, and accrues validator revenue. Does not create a lock or touch pool staking. |
| `withdraw_revenue` | Validator owner | **1 yocto** | `Promise` | `validator_id`. Verifies ownership through pool `get_owner_id()`, then transfers all direct-payment revenue for that validator to the validator owner. Catalog managers are not authorized. |
| `get_revenue_balance_for_validator` | Anyone | 0 | `NearToken` | `validator_id`. Returns currently withdrawable direct-payment revenue for the validator. |
| `get_purchase` | Anyone | 0 | `Purchase \| null` | `purchase_id`. Returns a direct payment purchase record. |
| `get_purchases` | Anyone | 0 | `Purchase[]` | `from_index`, `limit`. Lists direct payment purchases. |
| `get_purchases_for_account` | Anyone | 0 | `Purchase[]` | `account_id`, `from_index`, `limit`. Lists direct payment purchases by buyer. |
| `get_purchases_for_product` | Anyone | 0 | `Purchase[]` | `product_id`, `from_index`, `limit`. Lists direct payment purchases by product. |

Revenue withdrawal uses the same pool-owner callback pattern as catalog mutations, but remains owner-only: `withdraw_revenue` calls the pool `get_owner_id()`, then private `withdraw_revenue_after_get_owner` verifies the original caller is the pool owner, clears the validator revenue balance, and transfers the full balance.

---

## Subscriptions (`subscriptions.rs`)

Lifecycle RPCs (locking / renewal stays in **`lock`** above).

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `cancel_subscription` | Subscriber | **1 yocto** | — | **`subscription_id`** — set **`cancel_at_period_end`**; lock remains until **`lock.end_ns`**, then **`unlock`**. After **`end_ns`**, next **`lock`** starts a new period. |
| `resume_subscription` | Subscriber | **1 yocto** | — | **`subscription_id`** — clear **`cancel_at_period_end`** while **`Active`**, only before stored **`end_ns`** (current billing period). Fails after period end; use **`lock`** for a new period. Requires **`cancel_at_period_end == true`**. |
| `update_subscription` | Subscriber | **Attach delta NEAR for increases; 1 yocto otherwise** | **`PromiseOrValue<SubscriptionPlanChangeOutcome>`** | **`subscription_id`, `target_price_id`, `target_amount`** — unified plan update. Stake increases apply immediately after the same pre-user pipeline as **`lock`**; stake decreases are scheduled for the billing boundary, projected in views after `apply_ns`, and lazily committed by later mutations after validator settlement; price-only changes with unchanged stake apply immediately. |
| `get_subscriptions` | Anyone | 0 | `from_index`, `limit`. Lists subscriptions from the global subscription index. |
| `get_subscriptions_for_account` | Anyone | 0 | `account_id`, `from_index`, `limit`. Lists subscriptions by subscriber account. |
| `get_subscriptions_for_product` | Anyone | 0 | `product_id`, `from_index`, `limit`. Lists subscriptions by current product. |

---

## Unlock (`unlock.rs`)

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `unlock` | Lock owner | **1 yocto** | **`Promise`** | After **`block_timestamp >= lock.end_ns`**: pre-user settlement (**0–3**), then **`commit_share_exit`** (**5b**) — burns shares, queues **`pending_to_unstake`** and user tranches. Pool **`unstake`** for this exit is **not** called in the same transaction; it runs on the next **`lock`**, **`unlock`**, **`withdraw`**, or **`epoch_settle`** when the epoch slot is available. |

---

## Pool pipeline (`epoch.rs`)

Public **`epoch_stake` / `epoch_unstake` / `epoch_withdraw` / `refresh_validator_balance`** are **not** exposed. Pool work is driven by user flows below and optional **`epoch_settle`**.

**Per allowlisted pool (`validator_id` = pool contract account):**

- **`tx_status`**: **`Idle`** / **`Busy`** — one orchestrated pipeline at a time; **`Busy`** from entry until **`on_epoch_pipeline_terminal_release`** (**6**).
- **Per NEAR `epoch_height`**: at most **one** successful pool **`deposit_and_stake`** **or** **`unstake`** (or inline net-zero clear); updates **`last_settlement_epoch`**. Fresh-epoch no-pending and unstake-waiting settlement decisions also update **`last_settlement_epoch`**. A failed pool **`withdraw`** skips stake/unstake settlement and leaves the epoch retryable.
- **Fast path**: when **`last_settlement_epoch >= epoch_height`**, skip pool **`get_account`**, withdraw-if-ready, and net settle; jump to user tail (**4**) using cached **`total_staked_balance`**.
- **Full path**: pool **`get_account`** → optional withdraw (**2a–2c**) → **`try_epoch_stake_or_unstake`** on **existing** pending → user tail (**4**).
- **Unstake spacing**: another pool **`unstake`** requires **`validator_unstake_waiting_finished`** (`last_unstake_epoch` + **`epoch_unstake_settle_epochs`**).
- **Withdraw from pool** does **not** consume the stake/unstake epoch slot.

| Entry | `UserAction` tail | User tail |
|--------|-------------------|-----------|
| `lock` | `CommitLock` or `CommitRecurringSubscriptionLock` | Mint one-off lock or resolve recurring subscription lock after settlement (**5a**), then release with lock id (**6**) |
| `update_subscription` | `SubscriptionUpdate` | Update subscription lock or schedule deferred changes (**5d**), then release with outcome (**6**) |
| `stake` | `CommitFarmStake` | Farm stake after settlement (**5e**), then release with `FarmPosition` (**6**) |
| `unstake` | `FarmUnstakeQueue` | Farm share exit (**5f**), then terminal release (**6**) |
| `unlock` | `UnlockQueueUnstake` | Share exit only (**5b**), then terminal release (**6**) |
| `withdraw` (WASM) | `WithdrawUserTransfer` | Payout from claim bucket (**5c**), then terminal release (**6**) |
| `epoch_settle` | `SettleOnly` | No-op then **6** |

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `epoch_settle` | Any | **None** | **`Promise`** | **`validator_id`** — manual retry / advance pending stake or unstake; same rules as automatic flows. |

Pipeline steps and callbacks: [features/lazy-epoch-pipeline.md](features/lazy-epoch-pipeline.md).

---

## Private pool callbacks (`epoch.rs`, `lock.rs`, `unlock.rs`, `subscriptions.rs`)

**Not for users.** **`#[private]`** — only this contract account (promise continuations).

| Callback | Pipeline | Role |
|----------|----------|------|
| `on_epoch_settlement_after_pool_account` | **1** | After pool **`get_account`**: refresh **`total_staked_balance`**, optional **2a–2c**, then **3** for the original `UserAction`. |
| `on_epoch_withdraw_transfer_done` | **2b** | Credit **`pending_to_withdraw`** after pool **`withdraw`**. |
| `on_after_pool_withdraw_maybe_settle` | **2c** | After **2b**; continues through **3** → **4** for the original `UserAction`. |
| `on_deposit_and_stake` | **3b** | Stake callback; pending queue + **`last_settlement_epoch`**. |
| `on_unstake` | **3c** | Unstake callback; **`last_unstake_epoch`**, **`last_settlement_epoch`**. |
| `on_epoch_settlement_dispatch_continue` | **4** | Fan-out to **5a** / **5b** / **5c** / **5d** / **5e** / **5f** and choose a terminal or value-returning release callback. |
| `resolve_lock` | **5a** | One-off catalog lock mint (`lock.rs`). |
| `resolve_recurring_subscription_lock_after_settle` | **5a** | Recurring subscription renewal/new-period resolution after validator settlement (`lock.rs`). |
| `resolve_unlock` | **5b** | Share exit and lock status update (`unlock.rs`). |
| `on_withdraw_user_transfer_after_settle` | **5c** | Claim from `pending_to_claim` and transfer to user (`withdraw.rs`). |
| `on_subscription_update_after_settle` | **5d** | Subscription update after settlement (`subscriptions.rs`). |
| `resolve_farm_stake` | **5e** | Farm stake mint and reward-accounting update (`stake.rs`). |
| `resolve_farm_unstake` | **5f** | Farm share exit, reward settlement, and optional account roll-up (`stake.rs`). |
| `on_epoch_pipeline_terminal_release` | **6** | Set **`tx_status`** → **`Idle`** for no-value tails. |
| `on_epoch_pipeline_release_with_lock_id` | **6** | Release **`Busy`** and return `LockId`; refunds payable lock deposit if the tail fails. |
| `on_epoch_pipeline_release_with_subscription_update_outcome` | **6** | Release **`Busy`** and return `SubscriptionPlanChangeOutcome`; refunds attached update deposit if the tail fails. |
| `on_epoch_pipeline_release_with_farm_position` | **6** | Release **`Busy`** and return `FarmPosition`; refunds attached farm stake if the tail fails. |

---

## Withdrawals & claims (`withdraw.rs`)

| Method | Access | Deposit | Returns | Description |
|--------|--------|---------|---------|-------------|
| `withdraw` | User | **1 yocto** | **`Promise`** | JSON **`{ "validator_id": <AccountId> }`** — after shared settlement, claim epoch-eligible pending-unstake tranches from **`pending_to_claim`** and transfer NEAR to the caller. Settlement may first pull pool funds into `pending_to_claim` through `pending_to_withdraw` when the pool has withdrawable funds (see [features/lazy-epoch-pipeline.md](features/lazy-epoch-pipeline.md)). |
| `get_account_pending_unstake` | View | — | **`AccountPendingUnstakeView`** | JSON **`{ "account_id": <AccountId>, "validator_id": <AccountId> }`** — returns user pending-unstake tranches, total pending amount, epoch-eligible amount, validator claim bucket, and conservative withdrawable status for the UI withdraw flow. Missing user tranches return zero values instead of panicking. |

> **Note:** An owner-only **`sweep_stranded_withdraw_bucket`**-style cleanup (when no user pending-unstake tranche liability remains for the validator but **`pending_to_withdraw > 0`**) is described in [DESIGN.md](DESIGN.md) but **not** exposed in the current ABI.

---

## Governance (`governance.rs`)

**Caller:** **`owner_account_id`** unless noted. All attach **1 yocto**.

| Method | Description |
|--------|-------------|
| `propose_new_owner_account_id` | `new_owner_account_id: AccountId \| null`. |
| `accept_ownership` | **Proposed** account accepts (must match `proposed_new_owner_account_id`). |
| `set_guardians` | Replace **`guardians`** list. |
| `set_per_lock_storage_stake` | Per-lock storage surcharge config. |
| `set_per_farm_position_storage_stake` | Per-farm-position storage surcharge config. |
| `set_per_purchase_storage_stake` | Per-direct-purchase storage surcharge config. |
| `set_lock_bounds` | `min_lock_duration_ns`, `max_lock_duration_ns` (`U64`). |
| `set_min_lock_amount` | Minimum attach for locks; must be **≥ 1 NEAR** (`PROTOCOL_MIN_LOCK_AMOUNT_YOCTO` in `config.rs`). |
| `set_min_storage_deposit` | Minimum prepaid storage. |
| `set_epoch_unstake_settle_epochs` | Epochs to wait between unstake rounds / withdraw gates. |

---

## Pause (`pause.rs`)

| Method | Access | Deposit | Description |
|--------|--------|---------|-------------|
| `pause` | **Guardian** (or owner via `assert_guardian`) | **1 yocto** | Sets **`paused = true`**. |
| `unpause` | **Owner** | **1 yocto** | Sets **`paused = false`**. |

---

## Upgrade (`upgrade.rs`)

| Method | Access | Description |
|--------|--------|-------------|
| `migrate_state` | **`#[private]`** — contract account only | **`#[init(ignore_state)]`** — returns deserialized state after code upgrade (used by deploy script). |
| `get_version` | Any | Version string (see Views). |

---

## Catalog internal callbacks (`products.rs`)

**`#[private]`** — invoked only as promise callbacks after **`get_owner_id`** on the pool.

`create_product_after_get_owner`, `edit_product_after_get_owner`, `archive_product_after_get_owner`, `delete_product_after_get_owner`, `create_price_after_get_owner`, `edit_price_after_get_owner`, `archive_price_after_get_owner`, `delete_price_after_get_owner`, `unarchive_product_after_get_owner`, `unarchive_price_after_get_owner`, `set_product_default_price_after_get_owner`.

---

## Main types (contract JSON views)

- **`Config`** — [`../src/config.rs`](../src/config.rs): `owner_account_id`, `guardians`, lock/storage economics, `epoch_unstake_settle_epochs`, … **`min_lock_amount`** is the minimum attach for locks (including first delegation to an empty pool); governance may raise it but not below **`PROTOCOL_MIN_LOCK_AMOUNT_YOCTO`** (1 NEAR), enforced in `new` and `set_min_lock_amount`.
- **`Validator`** — [`../src/validators.rs`](../src/validators.rs): **`validator_id`** (pool contract account), accounting fields, pending buckets, **`tx_status`** (`Idle` \| `Busy`).
- **`Product`**, **`Price`**, **`PriceMetadata`**, **`Subscription`**, **`PendingSubscriptionUpdate`**, **`SubscriptionPlanChangeOutcome`**, **`Lock`**, **`Purchase`**, **`FarmPool`**, **`FarmPosition`**, **`FarmPositionView`**, **`FarmAccount`**, **`FarmAccountView`**, **`Account`**, **`StorageBalance`**, **`StorageBalanceBounds`** — [`../src/types.rs`](../src/types.rs), [`../src/accounts.rs`](../src/accounts.rs). **`Account`** is prepaid **`storage_deposit`** only (unlocked stake exits transfer directly to the user via **`withdraw`**).

## Events

All events are emitted as `EVENT_JSON` with `standard: "stake.dao"`, `version: "1.0.0"`, an `event` name, and a JSON `data` object.

Subscription lifecycle event payloads:

| Event | Data fields |
|-------|-------------|
| `subscription_cancel` | `account_id`, `subscription_id`, `product_id`, `price_id` |
| `subscription_resume` | `account_id`, `subscription_id`, `product_id`, `price_id` |
| `subscription_update` | `account_id`, `subscription_id`, `product_id`, `target_price_id`, `target_amount`, `update_kind` |
| `subscription_downgrade_prorate` | `account_id`, `subscription_id`, `product_id`, `price_id`, `lock_id`, `near_yocto` |

For other EVENT_JSON shapes and naming, see [`../src/events.rs`](../src/events.rs).

---

## Related

| Doc | Content |
|-----|---------|
| [features/lazy-epoch-pipeline.md](features/lazy-epoch-pipeline.md) | Per-epoch limits, fast path, promise pipeline **0–6**, callbacks |
| [DESIGN.md](DESIGN.md) | Architecture overview |
