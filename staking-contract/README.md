# staking-contract (`stake.dao`)

NEAR smart contract for pooled staking tied to NEAR AI **products** and **prices** (validator-owned catalog), with Stripe-style IDs. Catalog amounts are **NEAR (yocto)** only; there is no oracle or USD conversion path.

## Documentation

| File | Purpose |
|------|---------|
| [docs/API.md](docs/API.md) | Public contract methods (RPC-facing API) |
| [docs/DESIGN.md](docs/DESIGN.md) | Readable architecture summary + pointers |
| [docs/README.md](docs/README.md) | Documentation index |
| [docs/review/core-features.md](docs/review/core-features.md) | Reviewer quick reference (features + file priority) |
| [docs/features/lazy-epoch-pipeline.md](docs/features/lazy-epoch-pipeline.md) | Pool scheduling, settlement chain, callbacks (authoritative) |
| [docs/review/code-review.md](docs/review/code-review.md) | Pre-merge review checklist |
| [tests/README.md](tests/README.md) | Unit vs sandbox test layout |

## Build

From repo root (`house-of-stake-contracts/`):

```bash
cargo check -p staking-contract
# WASM (requires cargo-near, same as sibling crates):
cargo near build non-reproducible-wasm --manifest-path staking-contract/Cargo.toml
```

`build_all.sh` also builds this crate and copies `staking_contract.wasm` to `res/local/`.

For **sandbox integration tests** that exercise real pool cross-contract calls, build the mock pool (`../mock-staking-pool-contract`) as well: `make mock-staking-pool-contract` (repo root `Makefile`, alias `make mock-pool`) or run `build_all.sh`. Tests live in [`tests/sandbox_mock_pool.rs`](tests/sandbox_mock_pool.rs) (helpers in [`tests/mock_pool/mod.rs`](tests/mock_pool/mod.rs)) and load WASM from `res/local/`, `target/near/…`, or `target/wasm32-unknown-unknown/release/…`.

## User cadence (lazy pool pipeline)

Typical sequence after locks exist (no public `epoch_stake` / `epoch_unstake` / `epoch_withdraw` batch APIs; pool work is driven from user methods and optional **`epoch_settle`**; see [`docs/features/lazy-epoch-pipeline.md`](docs/features/lazy-epoch-pipeline.md)):

1. **`lock`** — Mints shares, queues `pending_to_stake`, then [`try_epoch_stake_or_unstake`](src/epoch.rs) runs after balance refresh (one pool **`deposit_and_stake`** or **`unstake`** per NEAR epoch, net of pending buckets).
2. User **`unlock`** — After lock period; refresh balance, queue unstake, then **`unstake`** / withdraw-from-pool as needed.
3. Wait **`epoch_unstake_settle_epochs`** (config) after each successful pool **`unstake`**.
4. User **`withdraw(validator_id)`** — May pull unstaked NEAR from the pool by moving `pending_to_withdraw` into the in-contract `pending_to_claim` bucket when allowed, then pro-rata claim and **transfer** that NEAR to the caller in one flow.

**Per pool and NEAR epoch (matches the staking pool contract):** the pool accepts **at most one** successful **`deposit_and_stake`** **or** **`unstake`** per `epoch_height` for that pool account. The contract records the last handled epoch in **`Validator.last_settlement_epoch`** after stake, unstake, net-zero settlement, no-pending settlement, or unstake-waiting settlement decisions, so a second pool-side settlement in the **same** epoch is skipped.

**Net settlement:** before calling the pool, the contract compares **`pending_to_stake`** and **`pending_to_unstake`** in yocto. It stakes only the excess stake, unstakes only the excess unstake, or (when the two are equal and non-zero) clears both buckets and user unstake liability **without** a pool mutating call, still bumping **`last_settlement_epoch`**. **`epoch_settle(validator_id)`** retries the same pipeline for manual advance. A fresh-epoch pipeline also bumps **`last_settlement_epoch`** when there is no pending stake/unstake or when queued unstake must wait for a later epoch. If pool withdraw fails, stake/unstake settlement is skipped and the epoch remains retryable. Withdraw-from-pool does **not** use this stake/unstake slot by itself.

## Implementation status (snapshot)

Implemented in code:

- Config, owner / guardians governance, pause, upgrade (`upgrade()` + `migrate_state`)
- On-contract validator **allowlist** (`add_validator`, `pause_validator`, `remove_validator`)
- Validator-owner **catalog** (`create_product`, `create_price`, …)
- Stripe-like deterministic IDs (`prod_*`, `price_*`, `lock_*`, `sub_*`)
- Share minting and lock pricing helpers ([`utils.rs`](src/utils.rs)) and NEAR-denominated `lock`
- Subscriptions keyed by `subscription_id`, with `(account_id, product_id)` as the active-product lookup and tier = [`Subscription::price_id`](src/types.rs): **`cancel_subscription`**, **`resume_subscription`**, **`update_subscription`** ([`subscriptions.rs`](src/subscriptions.rs)). On renewal with a scheduled decrease, **Phase B prorate** releases stake above the target amount into the normal unstake queue ([`subscriptions.rs`](src/subscriptions.rs) / [`lock.rs`](src/lock.rs) / [`unlock.rs`](src/unlock.rs)).
- `unlock` (user-driven pool unstake path); **`lock`** schedules refresh + net pool settle; **`withdraw`** may chain pool withdraw then pro-rata payout; **`epoch_settle`** retries settlement
- Pool callbacks in [`epoch.rs`](src/epoch.rs); **`storage_withdraw`**
- **EVENT_JSON** for lock/unlock, catalog, validators, epoch ops, claim/withdraw, pool withdraw-in ([`events.rs`](src/events.rs)) — `standard: "stake.dao"`, `version: "1.0.0"`, nested `data`
- **`get_products`**, **`get_product_default_price`**, catalog **`unarchive_*`**, **`set_product_default_price`**; **`lock`** accepts explicit **`price_id`** or **`product_id`** (uses **`Product.default_price_id`**) ([`products.rs`](src/products.rs), [`lock.rs`](src/lock.rs))

## Workspace

Listed as a member in [../Cargo.toml](../Cargo.toml).
