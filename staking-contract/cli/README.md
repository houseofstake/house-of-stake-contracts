# staking-cli

`staking-cli` is a small bootstrap CLI for `staking-contract` deployments. It replaces the
environment-variable-heavy shell flow for the first-pass staking use cases:

- `deploy`
- `configure`
- `verify`

Mutating commands default to dry-run. Pass `--send` to submit transactions. Mainnet mutations also
require `--yes-mainnet`.

## Build

```bash
cargo build -p staking-cli
```

## Examples

Build the test-feature contract WASM with the existing Makefile target:

```bash
make staking-contract-test
```

Environment config files live under `staking-contract/cli/config/`:

- `dev.testnet.json` targets `hos-e2e-0601144939.testnet`
- `qa.testnet.json` targets `stake-dao.testnet`
- `stg.mainnet.json` targets `stake-dao.near`
- `prod.mainnet.json` targets `stake.dao`

Fresh deploy configs include the storage stake fields passed to `new(config)`:

```json
{
  "init": {
    "per_lock_storage_stake": "10000000000000000000000",
    "per_farm_position_storage_stake": "5000000000000000000000",
    "per_purchase_storage_stake": "5000000000000000000000"
  }
}
```

Code-only deploy to the shared testnet account without running `migrate_state()`:

```bash
cargo run -p staking-cli -- deploy \
  --network testnet \
  --config staking-contract/cli/config/dev.testnet.json \
  --code-only \
  --test-feature \
  --send
```

Fresh deploy with `new(config)`:

```bash
cargo run -p staking-cli -- deploy \
  --network testnet \
  --config staking-contract/cli/config/dev.testnet.json \
  --fresh \
  --send
```

Configure validators and catalog entries from the config file:

```bash
cargo run -p staking-cli -- configure \
  --network testnet \
  --config staking-contract/cli/config/dev.testnet.json \
  --send
```

For existing catalog rows, include `product_id` and/or `price_id` in the config. `configure` will
then update product and price display fields in place. Price amount, type, billing period, and lock
factor are immutable; changing those fields requires creating a new price. To create a new price
under an existing product, include the existing `product_id` and omit `price_id` on the new price.
Price metadata can be updated, but clearing existing metadata to `null` is not supported by the
contract `edit_price` call; create a replacement price when metadata must be removed.

Verify deployment health:

```bash
cargo run -p staking-cli -- verify \
  --network testnet \
  --config staking-contract/cli/config/dev.testnet.json \
  --test-feature
```

The CLI supports `--network testnet` and `--network mainnet`. Mainnet deployment/configuration
requires `--yes-mainnet` in addition to `--send`.

Mainnet catalog calls must be signed by the validator pool owner or by a catalog manager already
granted on the staking contract. For staging, `nearai.pool.near` is owned by
`jasnah-treasury.sputnik-dao.near`, so `stg.mainnet.json` records that account on the validator
entry for the pool-owner grant context. The CLI still signs catalog calls with the configured
key-backed signer, `stake-dao.near`; before running staging catalog configuration, submit
`add_validator_catalog_manager` from the pool-owner DAO with
`catalog_manager_account_id: "stake-dao.near"`. For one-off pool-owner operations, set
`owner_account_id` on the individual product or pass an explicit `--signer`.
