# Code review notes: `feat/stake-dao` vs `main`

**Audience:** reviewers before merge to `main`  
**Baseline:** `main` @ `57482d6` (*Build v1.0.3 contract version.*)  
**Branch:** `feat/stake-dao` (review prep: `chore/stake-dao-review-prep`)

This is **review guidance**, not a security audit. The highest-risk surface is **`staking-contract`**: `epoch.rs`, `lock.rs`, `unlock.rs`, `withdraw.rs`, and `utils.rs` (share math / pricing) against [`features/lazy-epoch-pipeline.md`](../features/lazy-epoch-pipeline.md).

## What changed vs `main`

- **New crate:** `staking-contract` (`stake.dao`) — pooled staking, validator catalog, subscriptions, lazy pool pipeline.
- **Test support:** `mock-staking-pool-contract`, expanded `staking-contract/tests/`, workspace `Makefile` + CI workflows.
- **Sibling crates:** `common`, `lockup-contract`, `venear-contract`, `voting-contract` — mostly import ordering / formatting; tiny `voting-contract` migration glue (`sys`, `#[allow(dead_code)]` on legacy `OldConfig`).

Review energy should go to **new staking code and tests**, not re-litigating voting changes already on `main`.

## Strengths

- **Documented constraints:** `README.md`, `DESIGN.md`, and `features/lazy-epoch-pipeline.md` spell out per-epoch pool limits, net settle, fast-path rules, and the promise pipeline.
- **Modular layout:** pool promises in `epoch.rs`; user entrypoints in `lock` / `unlock` / `withdraw` / `subscriptions`.
- **Test matrix:** focused host unit modules plus `sandbox_mock_pool.rs` / `sandbox_epoch_settlement.rs` for cross-contract behavior.
- **CI:** build / format / clippy / test workflows; test job builds WASM via `build_all.sh` before `make test`.

## Risks and scrutiny areas

### 1. `epoch.rs` — callbacks and settlement

- **`Validator.tx_status`**, **`pending_to_*`**, **`user_pending_unstake`**, and share totals must stay consistent across every callback success/failure path; confirm **Busy** and **`epoch_settle`** retries cannot deadlock or double-apply.
- **Fast path** when `last_settlement_epoch >= epoch_height` skips pool `get_account` and uses cached **`total_staked_balance`** — verify no path desyncs shares from pool reality within the same epoch under concurrent users.
- **Net-zero pending** (matched stake/unstake cleared without a pool mutating call, epoch slot still consumed): confirm alignment with real staking-pool rules.

### 2. Economics and catalog

- **`min_lock_amount`** cannot go below **1 NEAR** (`PROTOCOL_MIN_LOCK_AMOUNT_YOCTO` in `config.rs`).
- **Subscriptions:** upgrade / downgrade / renewal proration — assert yocto conservation where required; see `subscription_lifecycle.rs` and the Subscriptions section in [`API.md`](../API.md).

### 3. Gas

- Long promise chains on `lock`, `unlock`, `withdraw`. Confirm `require_enough_gas_for_epoch_settlement` and documented minimum prepaid gas on hot paths.

### 4. Build and CI

- **`near-workspaces`** supports **linux-x86** and **darwin-arm** only; other hosts rely on **CI (Ubuntu)** for sandbox tests.
- Integration tests need **`cargo near`** + **`build_all.sh`** (plain `wasm32` build can fail deserialization on sandbox).

## Suggested follow-ups (non-blocking)

- Sandbox golden path: [`tests/sandbox_golden_path.rs`](../../tests/sandbox_golden_path.rs); subscription sandbox: [`tests/sandbox_subscription_e2e.rs`](../../tests/sandbox_subscription_e2e.rs).
- Calendar-accurate subscription billing vs linear-month helper.
- Stronger invariant tests (shares vs `total_staked_balance` vs user positions).

## Review order (files)

See [`core-features.md`](core-features.md) for priority table. Start with `epoch.rs`, then `utils.rs`, `lock.rs`, `subscriptions.rs`, `withdraw.rs`.

## Local verification

```bash
# From house-of-stake-contracts/
make check-staking-contract
make test-staking-contract   # needs near-workspaces-supported host + built WASM
cargo fmt --all -- --check
```

Full workspace `make test` matches CI when sandbox install succeeds.

---

*Refresh or remove after merge if you do not keep ephemeral review artifacts in-tree.*
