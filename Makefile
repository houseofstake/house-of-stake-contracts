# House of Stake per-contract NEAR WASM builds.

.DEFAULT_GOAL := help

.PHONY: help all-contracts \
	sandbox-staking-whitelist-contract venear-contract lockup-contract voting-contract \
	staking-contract staking-contract-test mock-staking-pool-contract \
	whitelist venear lockup voting staking staking-test mock-pool \
	check-sandbox-staking-whitelist-contract check-venear-contract check-lockup-contract \
	check-voting-contract check-staking-contract check-mock-staking-pool-contract \
	check-whitelist check-venear check-lockup check-voting check-staking check-mock-pool \
	test test-integration test-staking-contract test-staking

ROOT := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
RES_LOCAL := $(ROOT)res/local
INTEGRATION_TEST_FILES := $(sort $(notdir $(wildcard $(ROOT)integration-tests/tests/test_*.rs)))
INTEGRATION_TEST_ARGS := $(foreach test,$(patsubst %.rs,%,$(INTEGRATION_TEST_FILES)),--test $(test))

help:
	@echo "WASM builds (cargo near build non-reproducible-wasm; copies .wasm to res/local/):"
	@echo "  make sandbox-staking-whitelist-contract   (alias: make whitelist)"
	@echo "  make venear-contract                      (alias: make venear)"
	@echo "  make lockup-contract                      (alias: make lockup)"
	@echo "  make voting-contract                      (alias: make voting; sandbox feature for integration tests)"
	@echo "  make staking-contract                     (alias: make staking)"
	@echo "  make staking-contract-test                build test-feature WASM with mocked clock"
	@echo "  make mock-staking-pool-contract           (alias: make mock-pool) for staking-contract sandbox tests"
	@echo "  make all-contracts                        all deployable contract WASM artifacts"
	@echo ""
	@echo "Fast compile checks:"
	@echo "  make check-<name>   e.g. make check-staking-contract, make check-whitelist"
	@echo ""
	@echo "Tests:"
	@echo "  make test                                 run workspace tests and integration tests"
	@echo "  make test-integration                     run integration tests"
	@echo "  make test-staking-contract                run staking-contract test suite"
	@echo "  make test-staking                         alias for test-staking-contract"

sandbox-staking-whitelist-contract:
	cd "$(ROOT)sandbox-staking-whitelist-contract" && cargo near build non-reproducible-wasm
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/sandbox_staking_whitelist_contract/sandbox_staking_whitelist_contract.wasm" "$(RES_LOCAL)/"

venear-contract:
	cd "$(ROOT)venear-contract" && cargo near build non-reproducible-wasm
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/venear_contract/venear_contract.wasm" "$(RES_LOCAL)/"

lockup-contract:
	cd "$(ROOT)lockup-contract" && cargo near build non-reproducible-wasm
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/lockup_contract/lockup_contract.wasm" "$(RES_LOCAL)/"

voting-contract:
	cd "$(ROOT)voting-contract" && cargo near build non-reproducible-wasm --features sandbox
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/voting_contract/voting_contract.wasm" "$(RES_LOCAL)/"

staking-contract:
	cd "$(ROOT)staking-contract" && cargo near build non-reproducible-wasm
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/staking_contract/staking_contract.wasm" "$(RES_LOCAL)/"

staking-contract-test:
	cd "$(ROOT)staking-contract" && cargo near build non-reproducible-wasm --features test
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/staking_contract/staking_contract.wasm" "$(RES_LOCAL)/staking_contract_test.wasm"

mock-staking-pool-contract:
	cd "$(ROOT)mock-staking-pool-contract" && cargo near build non-reproducible-wasm
	mkdir -p "$(RES_LOCAL)"
	cp "$(ROOT)target/near/mock_staking_pool_contract/mock_staking_pool_contract.wasm" "$(RES_LOCAL)/"

all-contracts: sandbox-staking-whitelist-contract venear-contract lockup-contract voting-contract staking-contract mock-staking-pool-contract

whitelist: sandbox-staking-whitelist-contract
venear: venear-contract
lockup: lockup-contract
voting: voting-contract
staking: staking-contract
staking-test: staking-contract-test
mock-pool: mock-staking-pool-contract

check-sandbox-staking-whitelist-contract check-whitelist:
	cd "$(ROOT)" && cargo check -p sandbox-staking-whitelist-contract

check-venear-contract check-venear:
	cd "$(ROOT)" && cargo check -p venear-contract

check-lockup-contract check-lockup:
	cd "$(ROOT)" && cargo check -p lockup-contract

check-voting-contract check-voting:
	cd "$(ROOT)" && cargo check -p voting-contract

check-staking-contract check-staking:
	cd "$(ROOT)" && cargo check -p staking-contract

check-mock-staking-pool-contract check-mock-pool:
	cd "$(ROOT)" && cargo check -p mock-staking-pool-contract

test-staking-contract test-staking:
	$(MAKE) staking-contract staking-contract-test mock-staking-pool-contract
	cd "$(ROOT)" && cargo test -p staking-contract

test:
	$(MAKE) all-contracts staking-contract-test
	cd "$(ROOT)" && cargo test --workspace --exclude integration-tests
	$(MAKE) test-integration

test-integration:
	@if [ -z "$(strip $(INTEGRATION_TEST_ARGS))" ]; then echo "No integration tests matched"; exit 1; fi
	cd "$(ROOT)" && cargo test -p integration-tests $(INTEGRATION_TEST_ARGS)
