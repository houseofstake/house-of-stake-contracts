#!/usr/bin/env bash
#
# Build staking-contract WASM into res/local/staking_contract.wasm.
# This builds the normal contract, without the test feature.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$REPO_ROOT"

if command -v make >/dev/null 2>&1; then
  make staking-contract
else
  echo "make not found; run from repo root:" >&2
  echo "  cd staking-contract && cargo near build non-reproducible-wasm" >&2
  echo "  mkdir -p res/local" >&2
  echo "  cp target/near/staking_contract/staking_contract.wasm res/local/" >&2
  exit 1
fi

echo "Built:"
echo "  $REPO_ROOT/res/local/staking_contract.wasm"
