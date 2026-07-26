# Core features and files to review (`staking-contract` / stake.dao)

Quick reference for reviewers: what the contract does on-chain, and which source files to read first.

## Core features

1. **Validator allowlist** — Owner adds/pauses/removes staking pools; each row holds pool accounting (`validators.rs`, `governance.rs`).
2. **Catalog (NEAR-only)** — Validator owners and their configured catalog managers manage **products** and **prices** (yocto amounts, recurring vs one-off, billing hints) via pool-owner/owner-delegated callbacks (`products.rs`, `prices.rs`, `types.rs`).
3. **Locks** — Users call **`lock`**: mint internal shares, queue stake, enforce price vs locked amount × duration (`lock.rs`, `utils.rs`).
4. **Subscriptions** — One subscription per `(account, product)`; cancel, resume, update, getters (`subscriptions.rs`); **`lock`** and renewal prorate hook (`lock.rs`); unlock path (`unlock.rs`).
5. **Lazy pool pipeline** — No settlement operator role: **`deposit_and_stake` / `unstake` / withdraw** and balance refresh are driven from lock, unlock, claim, plus manual settle/retry as documented. **One** successful stake **or** unstake per pool per NEAR epoch; net settle on pending buckets (`epoch.rs`).
6. **Unlock → withdraw** — After lock end: unstake path, settle epochs, pull from pool when allowed, then user **`withdraw(validator_id)`** to receive NEAR (`unlock.rs`, `withdraw.rs`).
7. **Accounts & storage** — NEP-145-style registration, per-lock storage stake, `storage_withdraw` (`accounts.rs`, `lib.rs` state).
8. **Pause / upgrade** — Guardians pause; owner upgrades + migrate (`pause.rs`, `upgrade.rs`, `governance.rs`).
9. **Events** — `EVENT_JSON` for indexing (`events.rs`).

## Files to review (priority order)

| Priority | File(s) | Why |
|----------|---------|-----|
| **1** | `staking-contract/src/epoch.rs` | Cross-contract promises, callbacks, `last_settlement_epoch`, net settle, withdraw-before-unstake; highest correctness risk. |
| **2** | `staking-contract/src/utils.rs` | Share mint/burn math and NEAR price sufficiency checks. |
| **3** | `staking-contract/src/lock.rs` | Product/subscription **locks**, renewal, `finalize_lock`; synchronous mint on **non-WASM** targets (`testing_env!` / integration tests on the host triple), promise chain on **WASM**. |
| **3b** | `staking-contract/src/subscriptions.rs` | Subscription **lifecycle** RPCs, downgrade prorate at renewal, billing month helper. |
| **4** | `staking-contract/src/withdraw.rs` | Claim batches, pool withdraw chaining, user liability vs pool state. |
| **5** | `staking-contract/src/unlock.rs` | Unlock entry, Busy/Idle, interaction with epoch pipeline. |
| **6** | `staking-contract/src/validators.rs` | `Validator` invariants: pending buckets, `tx_status`, epochs, shares. |
| **7** | `staking-contract/src/types.rs` | Data model definitions used everywhere. |
| **8** | `staking-contract/src/products.rs`, `prices.rs` | Catalog auth (`get_owner_id`), archive/delete rules. |
| **9** | `staking-contract/src/lib.rs`, `config.rs` | Global config and storage keys. |
| **10** | `staking-contract/tests/sandbox_mock_pool.rs`, `tests/mock_pool/mod.rs` | End-to-end-ish behavior vs mock pool. |

## Design docs (read before deep line review)

1. [`features/lazy-epoch-pipeline.md`](../features/lazy-epoch-pipeline.md) — Pool scheduling and settlement rules
2. [`README.md`](../../README.md) — User cadence and status snapshot
3. [`API.md`](../API.md) — Public RPC-facing methods

## Secondary (workspace / integration)

- `mock-staking-pool-contract/src/lib.rs` — Mock pool for sandbox-style tests  
- `integration-tests/tests/test_staking_contract.rs` — Workspace integration coverage  
- `.github/workflows/` — CI merge gate  

Voting / `common` changes on a branch that already includes v1.0.3 on `main` are typically small relative to the staking crate.
