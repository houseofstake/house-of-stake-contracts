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

Grant one catalog manager for a validator from the validator pool owner account:

```bash
cargo run -p staking-cli -- add-catalog-manager \
  --network mainnet \
  --account stake-dao.near \
  --validator-id nearai.pool.near \
  --catalog-manager-account-id stake-dao.near \
  --owner jasnah-treasury.sputnik-dao.near \
  --send \
  --yes-mainnet
```

The same grant can be made as part of `configure` by adding managers to the validator config:

```json
{
  "validators": [
    {
      "validator_id": "nearai.pool.near",
      "owner_account_id": "jasnah-treasury.sputnik-dao.near",
      "catalog_manager_account_ids": ["stake-dao.near"]
    }
  ]
}
```

For existing catalog rows, include `product_id` and/or `price_id` in the config. `configure` will
then update product and price display fields in place. Price amount, type, billing period, and lock
factor are immutable; changing those fields requires creating a new price. To create a new price
under an existing product, include the existing `product_id` and omit `price_id` on the new price.
Price metadata can be updated, but clearing existing metadata to `null` is not supported by the
contract `edit_price` call; create a replacement price when metadata must be removed.

Catalog product and price calls are signed in this order:

1. `--signer`
2. product-level `catalog_manager_account_id`
3. product-level `owner_account_id` (legacy signer override)
4. the first validator-level `catalog_manager_account_ids` entry for the product's validator
5. `staking.signer_account_id`

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
entry for the pool-owner grant context. Add `catalog_manager_account_ids` to the validator config,
or run `add-catalog-manager`, before relying on `stake-dao.near` to manage catalog rows. If the
validator owner is a DAO, submit the same contract call through the DAO proposal flow; direct
keychain signing only works for accounts that have a usable key.
