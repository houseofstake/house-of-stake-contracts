#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

RELEASE_DIR="res/release"
BACKUP_ROOT="$RELEASE_DIR/backups"
BACKUP_DIR="$BACKUP_ROOT/$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$RELEASE_DIR"

build_contract() {
    local manifest_path="$1"

    cargo near build reproducible-wasm --manifest-path "$manifest_path"
}

build_contract venear-contract/Cargo.toml
build_contract lockup-contract/Cargo.toml
build_contract voting-contract/Cargo.toml
build_contract staking-contract/Cargo.toml

shopt -s nullglob
existing_wasms=("$RELEASE_DIR"/*.wasm)
if ((${#existing_wasms[@]} > 0)); then
    mkdir -p "$BACKUP_DIR"
    cp -p "${existing_wasms[@]}" "$BACKUP_DIR/"
    echo "Backed up existing release WASM files to $BACKUP_DIR"
fi
shopt -u nullglob

cp target/near/venear_contract/venear_contract.wasm "$RELEASE_DIR/"
cp target/near/lockup_contract/lockup_contract.wasm "$RELEASE_DIR/"
cp target/near/voting_contract/voting_contract.wasm "$RELEASE_DIR/"
cp target/near/staking_contract/staking_contract.wasm "$RELEASE_DIR/"

release_artifacts=(
    "$RELEASE_DIR/venear_contract.wasm"
    "$RELEASE_DIR/lockup_contract.wasm"
    "$RELEASE_DIR/voting_contract.wasm"
    "$RELEASE_DIR/staking_contract.wasm"
)

echo "Built reproducible release WASM artifacts in $RELEASE_DIR:"
printf '  %s\n' "${release_artifacts[@]}"
