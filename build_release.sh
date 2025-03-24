#!/bin/bash
set -e

cd $(dirname $0)
mkdir -p res/release

pushd venear-contract
cargo near build reproducible-wasm
popd

pushd lockup-contract
cargo near build reproducible-wasm
popd

pushd voting-contract
cargo near build reproducible-wasm
popd
cp target/near/venear_contract/venear_contract.wasm res/release/
cp target/near/lockup_contract/lockup_contract.wasm res/release/
cp target/near/voting_contract/voting_contract.wasm res/release/
