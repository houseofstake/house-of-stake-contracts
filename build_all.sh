#!/bin/bash
set -e

cd $(dirname $0)
mkdir -p res/local

RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/venear_lockup_contract.wasm res/local/
cp target/wasm32-unknown-unknown/release/venear_contract.wasm res/local/

