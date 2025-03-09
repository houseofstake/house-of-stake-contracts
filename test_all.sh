#!/bin/bash
set -e

cd $(dirname $0)
./build_all.sh

# Run tests
cargo test -- --nocapture
