# Production Stake DAO Fresh Deployment

This runbook deploys the production staking contract on mainnet, grants
`ironbuild.near` catalog-manager rights, has `ironbuild.near` configure the NEAR
AI products and prices, and then transfers contract ownership to
`hos-root.sputnik-dao.near` after verification.

## Catalog Manager

A catalog manager is an account delegated by the validator owner to manage
products and prices for one validator on the staking contract. Catalog managers
can create and update catalog rows, but they do not get broader staking,
settlement, revenue, governance, or contract-owner permissions.

This delegation is required because products and prices belong to a validator
pool. The staking contract verifies catalog mutations against the validator pool
owner, and `nearai.pool.near` is owned by `jasnah-treasury.sputnik-dao.near`.
Granting `ironbuild.near` as catalog manager lets the deployment operator set up
NEAR AI catalog entries without needing the DAO to submit every product or price
change.

## Accounts

| Role | Account |
| --- | --- |
| Network | `mainnet` |
| Production contract | `stake.dao` |
| Initial contract owner | `stake.dao` |
| Final contract owner | `hos-root.sputnik-dao.near` |
| Validator pool | `nearai.pool.near` |
| Validator owner DAO | `jasnah-treasury.sputnik-dao.near` |
| Catalog manager | `ironbuild.near` |
| Config | `staking-contract/cli/config/prod.mainnet.json` |
| WASM | `res/release/staking_contract.wasm` |

## 1. Confirm The Commit

```bash
git switch feat/stake-v1
git pull --ff-only origin feat/stake-v1
git status -sb
git log -1 --oneline
```

Deploy only from the intended commit and a clean tree.

## 2. Build Release WASM

```bash
make build-release
ls -lh res/release/staking_contract.wasm
```

## 3. Review The Production Config

Read the config that will drive contract initialization and catalog setup:

```bash
cat staking-contract/cli/config/prod.mainnet.json
```

Confirm it points to:

- staking contract: `stake.dao`
- initial owner: `stake.dao`
- validator pool: `nearai.pool.near`
- validator owner: `jasnah-treasury.sputnik-dao.near`
- expected NEAR AI products and prices

## 4. Fresh Deploy `stake.dao`

```bash
cargo run -p staking-cli -- deploy \
  --network mainnet \
  --config staking-contract/cli/config/prod.mainnet.json \
  --wasm res/release/staking_contract.wasm \
  --fresh \
  --send \
  --yes-mainnet
```

Equivalent NEAR CLI command:

```bash
near --quiet contract deploy stake.dao \
  use-file res/release/staking_contract.wasm \
  with-init-call new \
  json-args '{
    "config": {
      "owner_account_id": "stake.dao",
      "proposed_new_owner_account_id": null,
      "guardians": [],
      "min_lock_duration_ns": "1",
      "max_lock_duration_ns": "63072000000000000",
      "epoch_unstake_settle_epochs": 4,
      "min_storage_deposit": "10000000000000000000000",
      "per_lock_storage_stake": "10000000000000000000000",
      "per_farm_position_storage_stake": "5000000000000000000000",
      "per_purchase_storage_stake": "5000000000000000000000",
      "min_lock_amount": "1000000000000000000000000"
    }
  }' \
  prepaid-gas '100.0 Tgas' \
  attached-deposit '0 NEAR' \
  network-config mainnet \
  sign-with-keychain \
  send
```

## 5. Allowlist The Validator

```bash
near --quiet contract call-function as-transaction stake.dao \
  add_validator \
  json-args '{"validator_id":"nearai.pool.near"}' \
  prepaid-gas '50.0 Tgas' \
  attached-deposit '1 yoctoNEAR' \
  sign-as stake.dao \
  network-config mainnet \
  sign-with-keychain \
  send
```

## 6. DAO Proposal: Grant `ironbuild.near`

`jasnah-treasury.sputnik-dao.near` owns `nearai.pool.near`, so it must execute
the catalog-manager grant on `stake.dao`. Submit the proposal through a DAO
member account with permission to add proposals:

```bash
cargo run -p staking-cli -- propose-add-catalog-manager \
  --network mainnet \
  --config staking-contract/cli/config/prod.mainnet.json \
  --dao jasnah-treasury.sputnik-dao.near \
  --proposer <dao-member-account> \
  --validator-id nearai.pool.near \
  --catalog-manager-account-id ironbuild.near \
  --send \
  --yes-mainnet
```

Dry-run without `--send` first if you want to inspect the payload. The command
prints the wrapped staking-contract call, which should match:

```json
{
  "receiver_id": "stake.dao",
  "method_name": "add_validator_catalog_manager",
  "args": {
    "validator_id": "nearai.pool.near",
    "catalog_manager_account_id": "ironbuild.near"
  },
  "deposit": "1",
  "gas": "100000000000000"
}
```

It also prints the full Sputnik DAO `add_proposal` arguments. The proposal kind
must be `FunctionCall`, with `receiver_id` set to `stake.dao`, one action named
`add_validator_catalog_manager`, and base64-encoded action args matching the
JSON above. The command reads `jasnah-treasury.sputnik-dao.near.get_policy()`
and attaches the exact `proposal_bond` required by the DAO policy.

After the proposal executes, confirm:

```bash
near --quiet contract call-function as-read-only stake.dao \
  get_validator \
  json-args '{"validator_id":"nearai.pool.near"}' \
  network-config mainnet \
  now
```

Expected output includes:

```json
"catalog_manager_account_ids": ["ironbuild.near"]
```

## 7. Catalog Manager Configures Products And Prices

This step is done by `ironbuild.near`, after the DAO proposal grants it
catalog-manager rights.

Dry-run first:

```bash
cargo run -p staking-cli -- configure \
  --network mainnet \
  --config staking-contract/cli/config/prod.mainnet.json \
  --signer ironbuild.near \
  --yes-mainnet
```

Send after the dry-run shows catalog calls signed as `ironbuild.near`:

```bash
cargo run -p staking-cli -- configure \
  --network mainnet \
  --config staking-contract/cli/config/prod.mainnet.json \
  --signer ironbuild.near \
  --send \
  --yes-mainnet
```

## 8. Verify

```bash
cargo run -p staking-cli -- verify \
  --network mainnet \
  --config staking-contract/cli/config/prod.mainnet.json
```

```bash
near --quiet contract call-function as-read-only stake.dao \
  get_products \
  json-args '{"from_index":0,"limit":20}' \
  network-config mainnet \
  now
```

## 9. Transfer Contract Ownership

Do this only after deployment and catalog verification are complete.

First, `stake.dao` proposes `hos-root.sputnik-dao.near` as the new owner:

```bash
near --quiet contract call-function as-transaction stake.dao \
  propose_new_owner_account_id \
  json-args '{"new_owner_account_id":"hos-root.sputnik-dao.near"}' \
  prepaid-gas '30.0 Tgas' \
  attached-deposit '1 yoctoNEAR' \
  sign-as stake.dao \
  network-config mainnet \
  sign-with-keychain \
  send
```

Then `hos-root.sputnik-dao.near` accepts ownership through its DAO execution
flow by calling `accept_ownership` on `stake.dao`:

```json
{
  "receiver_id": "stake.dao",
  "method_name": "accept_ownership",
  "args": {},
  "deposit": "1",
  "gas": "30000000000000"
}
```

Confirm final owner:

```bash
near --quiet contract call-function as-read-only stake.dao \
  get_config \
  json-args '{}' \
  network-config mainnet \
  now
```

Expected output includes:

```json
"owner_account_id": "hos-root.sputnik-dao.near"
```
