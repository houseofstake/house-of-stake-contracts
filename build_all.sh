#!/bin/bash
set -e

cd $(dirname $0)
mkdir -p res/local

pushd venear-contract
cargo near build non-reproducible-wasm --no-abi
popd
cp target/near/venear_contract/venear_contract.wasm res/local/

pushd lockup-contract
cargo near build non-reproducible-wasm --no-abi
popd
cp target/near/lockup_contract/lockup_contract.wasm res/local/
