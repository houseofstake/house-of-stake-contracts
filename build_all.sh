#!/bin/bash
set -e

cd $(dirname $0)
mkdir -p res/local

pushd sandbox-staking-whitelist-contract
cargo near build non-reproducible-wasm
popd
cp target/near/sandbox_staking_whitelist_contract/sandbox_staking_whitelist_contract.wasm res/local/

pushd venear-contract
cargo near build non-reproducible-wasm
popd
cp target/near/venear_contract/venear_contract.wasm res/local/

pushd lockup-contract
cargo near build non-reproducible-wasm
popd
cp target/near/lockup_contract/lockup_contract.wasm res/local/

pushd voting-contract
cargo near build non-reproducible-wasm
popd
cp target/near/voting_contract/voting_contract.wasm res/local/
