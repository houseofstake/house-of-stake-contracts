#!/usr/bin/env bash
#
# Upgrade the existing testnet staking-contract to the latest local staking farm WASM.
#
# Defaults target the shared E2E testnet contract documented in
# staking-contract/docs/operations/testnet-contract-snapshot.md.
#
# Preview:
#   ./staking-contract/scripts/upgrade_testnet_staking_farm.sh
#
# Execute:
#   EXECUTE=1 ./staking-contract/scripts/upgrade_testnet_staking_farm.sh
#
# Optional environment:
#   STAKING_ACCOUNT_ID=hos-e2e-0601144939.testnet
#   OWNER_ACCOUNT_ID=$STAKING_ACCOUNT_ID
#   CHAIN_ID=testnet
#   STAKING_WASM=res/local/staking_contract.wasm
#   BUILD_WASM=1
#   RUN_TESTS=0
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

: "${CHAIN_ID:=testnet}"
: "${STAKING_ACCOUNT_ID:=hos-e2e-0601144939.testnet}"
: "${OWNER_ACCOUNT_ID:=$STAKING_ACCOUNT_ID}"
: "${STAKING_WASM:=$REPO_ROOT/res/local/staking_contract.wasm}"
: "${BUILD_WASM:=1}"
: "${RUN_TESTS:=0}"
: "${EXECUTE:=0}"

if [[ "$CHAIN_ID" != "testnet" ]]; then
  echo "Refusing to run: this script is testnet-only. Set CHAIN_ID=testnet." >&2
  exit 1
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 not found in PATH." >&2
    exit 1
  fi
}

require_command near
require_command jq

run() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  if [[ "$EXECUTE" == "1" ]]; then
    "$@"
  fi
}

view_json() {
  local contract_id="$1"
  local method_name="$2"
  local args_json="$3"

  near --quiet contract call-function as-read-only "$contract_id" "$method_name" \
    json-args "$args_json" network-config "$CHAIN_ID" now
}

wasm_hash() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$STAKING_WASM" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$STAKING_WASM" | awk '{print $1}'
  else
    echo "sha256 unavailable"
  fi
}

if [[ "$RUN_TESTS" == "1" ]]; then
  require_command cargo
  echo "== Running staking-contract tests =="
  cargo test -p staking-contract
  echo
fi

if [[ "$BUILD_WASM" == "1" ]]; then
  echo "== Building latest staking-contract WASM =="
  "$REPO_ROOT/staking-contract/scripts/build_staking_wasm.sh"
  echo
fi

if [[ ! -f "$STAKING_WASM" ]]; then
  echo "Missing staking WASM: $STAKING_WASM" >&2
  echo "Run with BUILD_WASM=1 or build it with: $REPO_ROOT/staking-contract/scripts/build_staking_wasm.sh" >&2
  exit 1
fi

echo "== Pre-upgrade on-chain checks =="
before_version="$(view_json "$STAKING_ACCOUNT_ID" get_version '{}')"
config_json="$(view_json "$STAKING_ACCOUNT_ID" get_config '{}')"
on_chain_owner="$(jq -er '.owner_account_id' <<<"$config_json")"

echo "Network:          $CHAIN_ID"
echo "Contract:         $STAKING_ACCOUNT_ID"
echo "Current version:  $before_version"
echo "On-chain owner:   $on_chain_owner"
echo "Signer:           $OWNER_ACCOUNT_ID"
echo "WASM:             $STAKING_WASM"
echo "WASM sha256:      $(wasm_hash)"
echo

if [[ "$OWNER_ACCOUNT_ID" != "$on_chain_owner" ]]; then
  echo "OWNER_ACCOUNT_ID must match get_config.owner_account_id for upgrade()." >&2
  exit 1
fi

if [[ "$EXECUTE" != "1" ]]; then
  echo "Preview only. Re-run with EXECUTE=1 to send the upgrade transaction."
  echo
fi

echo "== Owner-gated upgrade() =="
run near --quiet contract call-function as-transaction "$STAKING_ACCOUNT_ID" upgrade \
  file-args "$STAKING_WASM" \
  prepaid-gas '300.0 Tgas' attached-deposit '0 NEAR' \
  sign-as "$OWNER_ACCOUNT_ID" network-config "$CHAIN_ID" sign-with-keychain send

if [[ "$EXECUTE" == "1" ]]; then
  echo
  echo "== Post-upgrade checks =="
  view_json "$STAKING_ACCOUNT_ID" get_version '{}'
  view_json "$STAKING_ACCOUNT_ID" get_config '{}'
else
  echo
  echo "No transaction sent."
fi
