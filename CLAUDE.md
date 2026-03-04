# CLAUDE.md — House of Stake Contracts

This file provides context for AI assistants working on this repository.

## Project Overview

House of Stake is a NEAR Protocol smart contract suite implementing a vote-escrow staking system. Users lock NEAR tokens to receive **veNEAR** (vote-escrowed NEAR), which grants governance voting power that grows over time. The system consists of three main contracts plus shared libraries.

- **Repository**: https://github.com/fastnear/house-of-stake-contracts
- **Current version**: 1.0.2
- **License**: MIT OR Apache-2.0
- **Authors**: Fastnear Inc

---

## Repository Structure

```
house-of-stake-contracts/
├── common/                              # Shared types, events, utils
├── merkle-tree/                         # Persistent Merkle Tree implementation
├── venear-contract/                     # Main veNEAR contract (upgradable)
├── lockup-contract/                     # Per-user lockup contracts (non-upgradable)
├── voting-contract/                     # Governance voting contract
├── sandbox-staking-whitelist-contract/  # Test-only mock contract
├── integration-tests/                   # near-workspaces integration tests
├── scripts/                             # Deployment and testnet scripts
├── res/
│   ├── local/                           # Non-reproducible WASM (from build_all.sh)
│   └── release/                         # Reproducible WASM (from build_release.sh)
├── Cargo.toml                           # Workspace root
├── rust-toolchain.toml                  # Pins Rust stable channel
├── build_all.sh                         # Local dev build
├── build_release.sh                     # Docker reproducible build
├── test_all.sh                          # Build + run all tests
├── README.md                            # Project overview
└── API.md                               # Full contract API reference
```

---

## Toolchain & Dependencies

- **Rust**: stable channel (currently 1.85.1, pinned in `rust-toolchain.toml`)
- **Edition**: 2021
- **Key dependencies**:
  - `near-sdk = "=5.9.0"` (features: `wee_alloc`, `unstable`)
  - `serde_json = "1.0"` (feature: `preserve_order`)
  - `uint = "0.10.0"` — multi-precision integer arithmetic
  - `hex = "0.4.3"`
- **Dev/test dependencies**:
  - `near-workspaces = "0.18"` — sandbox NEAR environment
  - `tokio = "1"` (full features)
  - `sha2 = "0.10.8"`

---

## Build Commands

### Local development build (non-reproducible)
```bash
./build_all.sh
```
Compiles all four contracts using `cargo near build non-reproducible-wasm` and copies WASMs to `res/local/`.

### Release build (reproducible, Docker required)
```bash
./build_release.sh
```
Uses Docker image `sourcescan/cargo-near:0.13.4-rust-1.85.1` for verifiable reproducible builds. Output goes to `res/release/`.

### Run all tests
```bash
./test_all.sh
```
Runs `build_all.sh` first, then executes `cargo test -- --nocapture`.

### Run tests directly (after building)
```bash
cargo test -- --nocapture
```

### Run a single test
```bash
cargo test <test_name> -- --nocapture
```

---

## Contract Architecture

### `venear-contract/` — veNEAR Contract (main, upgradable)

The central contract. Tracks all accounts and their veNEAR balances in a Merkle tree.

**Key responsibilities:**
- Deploys individual lockup contracts for users
- Tracks delegated veNEAR balances
- Emits Merkle tree snapshots for governance voting
- Implements NEP-141 fungible token interface (non-transferable)
- Pause/unpause mechanism for emergencies

**Key modules:**
| File | Purpose |
|------|---------|
| `lib.rs` | Contract struct, `new()` init |
| `account.rs` | Account data and internal structures |
| `lockup.rs` | Lockup contract deployment |
| `delegation.rs` | veNEAR delegation to other accounts |
| `snapshot.rs` | Merkle tree snapshot retrieval |
| `token.rs` | NEP-141 fungible token interface |
| `governance.rs` | Owner/admin methods |
| `pause.rs` | Emergency pause |
| `upgrade.rs` | Contract upgrade via `sys::value_return` |

**Contract state:**
```rust
pub struct Contract {
    tree: MerkleTree<VAccount, VGlobalState>,
    accounts: LookupMap<AccountId, VAccountInternal>,
    config: Config,
    paused: bool,
}
```

---

### `lockup-contract/` — User Lockup Contract (non-upgradable)

One deployed per user. Holds locked NEAR and reports balance to veNEAR contract.

**Critical design constraint: these contracts are intentionally non-upgradable.** The veNEAR owner cannot take over lockup contracts. Once deployed, a lockup contract's code cannot be changed.

**Key modules:**
| File | Purpose |
|------|---------|
| `lib.rs` | Contract struct and core methods |
| `owner.rs` | Owner staking operations |
| `owner_callbacks.rs` | Async callbacks for staking |
| `venear.rs` | Integration callbacks with veNEAR |
| `getters.rs` | View methods |
| `transfer.rs` | Transfer functionality |

---

### `voting-contract/` — Governance Voting (upgradable)

Manages proposals and collects votes using Merkle proof-based verification.

**Key modules:**
| File | Purpose |
|------|---------|
| `proposal.rs` | Proposal creation and lifecycle |
| `votes.rs` | Vote casting and aggregation |
| `reviewer.rs` | Reviewer approval workflow |
| `governance.rs` | Owner/admin methods |

**Voting flow:**
1. A proposer creates a proposal (requires base proposal fee)
2. A reviewer approves/rejects it
3. Voting opens — users submit Merkle proofs of their veNEAR balance at snapshot
4. Votes aggregated by option

---

### `merkle-tree/` — Persistent Merkle Tree

Core data structure used by veNEAR contract to commit account state. Supports:
- Persistent storage via NEAR SDK `LookupMap`
- Block-level snapshots
- Merkle proof generation and verification
- Generic over data type and global state type

---

### `common/` — Shared Types and Utilities

Used by all contracts. Key types:

| Type | Description |
|------|-------------|
| `VenearBalance` | Holds `near_balance` + `extra_venear_balance` (grows over time) |
| `U256`, `U384` | Multi-precision integers via `uint` crate |
| `TimestampNs` | Nanosecond timestamp (serializes as string in JSON) |
| `Version` | Monotonically increasing `u64` |
| `VenearGrowthConfig` | Growth rate configuration for veNEAR accrual |

Events follow NEP-141/NEAR event standards and are emitted in `events.rs`.

---

## Key Design Patterns

### Versioned state with enums
State types use versioned enums (e.g., `VAccount`, `VGlobalState`) to enable future migrations without breaking Borsh-serialized storage.

### `#[near]` attribute macro
All contract state, argument types, and return types use `#[near(serializers=[borsh, json])]` or `#[near(contract_state)]` from `near-sdk` v5.

### Overflow-checked arithmetic
All build profiles (`dev`, `test`, `release`) have `overflow-checks = true`. Never use unchecked arithmetic.

### Release profile
```toml
[profile.release]
codegen-units = 1
opt-level = "s"   # Size-optimized for WASM
lto = true
debug = false
panic = "abort"
overflow-checks = true
```

### Callbacks
Cross-contract calls use the `#[near]` callback pattern. Callback methods are typically prefixed with `on_` or `callback_`.

### Storage fees
Contracts charge storage fees for new accounts and proposals to prevent spam.

---

## Integration Tests

Located in `integration-tests/tests/`. Uses `near-workspaces` to spin up a local NEAR sandbox.

| Test file | Coverage |
|-----------|---------|
| `test_venear.rs` | veNEAR: lockup deployment, delegation, snapshots, proofs |
| `test_lockup.rs` | Lockup: staking, withdrawal, balance tracking |
| `test_voting.rs` | Voting: proposals, Merkle proofs, vote aggregation |

**Test setup pattern:**
- `VenearTestWorkspace` builder in `setup/mod.rs` sets up all contracts
- Async tests use `#[tokio::test]`
- Helper `assert_almost_eq` for near-exact numerical comparisons

All integration tests require the WASM binaries to exist in `res/local/` — always run `./build_all.sh` before running tests.

---

## Deployment

### Environment variables for `scripts/deploy_all.sh`

| Variable | Default | Description |
|----------|---------|-------------|
| `ROOT_ACCOUNT_ID` | required | Root NEAR account |
| `CHAIN_ID` | `testnet` | `testnet` or `mainnet` |
| `CONTRACTS_SOURCE` | `local` | `local` or `release` |
| `UNLOCK_DURATION_SEC` | `600` | Lock duration in seconds |
| `VOTING_DURATION_SEC` | `600` | Voting period in seconds |

### Deployment creates:
1. veNEAR contract account (~2.4 NEAR)
2. Voting contract account (~2.3 NEAR)
3. Owner, guardian, lockup deployer sub-accounts

### Growth rate
The veNEAR growth rate denominator **must** be `10^30` (10^9 for nanoseconds × 10^21 for milliNEAR). This is enforced in `venear-contract/src/lib.rs`.

### Scripts
- `scripts/deploy_all.sh` — full ecosystem deployment
- `scripts/test_all.sh` — end-to-end testnet flow
- `scripts/lock_near.sh` — lock NEAR tokens
- `scripts/delegate.sh` — delegate veNEAR
- `scripts/create_proposal.sh` / `approve_proposal.sh` / `vote.sh` — governance flow

---

## Development Conventions

### Naming
- Contract files: snake_case module names matching their responsibility
- Test helpers: descriptive, e.g., `create_account`, `lock_near`, `assert_almost_eq`
- Callback methods: prefixed with `on_` or `callback_`

### Error handling
- Use `require!(condition, "message")` for precondition checks (panics on failure)
- Use `env::panic_str("message")` for explicit panics
- Never use `unwrap()` in contract code — always use `expect()` with a message or `require!`

### Serialization
- All public-facing types must implement both Borsh (storage) and JSON (external API) serialization using `#[near(serializers=[borsh, json])]`
- Use `U128` / `U64` from `near_sdk::json_types` for large numbers in JSON (they serialize as strings)

### Testing
- Add integration tests in `integration-tests/tests/` for cross-contract behavior
- Add unit tests in `#[cfg(test)]` modules inside individual contract files for pure logic
- All tests must pass before committing

### Security constraints
- **Never** make lockup contracts upgradable
- **Never** allow the veNEAR owner to drain lockup contracts
- All arithmetic must be overflow-checked
- Cross-contract calls must handle failure callbacks

---

## Versioning

Version is defined once in `[workspace.package]` in the root `Cargo.toml` and inherited by all crates. To bump the version, update only `Cargo.toml` at the root.

WASM release artifacts are versioned in `res/release/` subdirectories (e.g., `1_0_0/`, `1_0_1/`).

---

## Additional Documentation

- `README.md` — Project overview, build/test/deploy instructions
- `API.md` — Full contract API: all methods, parameters, return types, and structures
- `audit.pdf` — Third-party security audit report
