#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

: ${ROOT_ACCOUNT_ID:=$1}

# Fail if the root account ID is not set
if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Usage: $0 root_account_id"
  echo "Please set the root account ID."
  exit 1
fi

: "${CHAIN_ID:=testnet}"

# Shorter name, so we can fit more
: "${VENEAR_ACCOUNT_ID:=v.$ROOT_ACCOUNT_ID}"
: "${APPROVER_ACCOUNT_ID:=approver.$ROOT_ACCOUNT_ID}"
: "${VOTING_ACCOUNT_ID:=vote.$ROOT_ACCOUNT_ID}"
: "${OWNER_ACCOUNT_ID:=owner.$ROOT_ACCOUNT_ID}"
: "${LOCKUP_DEPLOYER_ACCOUNT_ID:=lockup-deployer.$ROOT_ACCOUNT_ID}"

export CURRENT_TIMESTAMP=$(date +%s)
: "${ACCOUNT_ID:=acc-$CURRENT_TIMESTAMP.$CHAIN_ID}"

echo "Creating account $ACCOUNT_ID"
# near account create-account sponsor-by-faucet-service $ACCOUNT_ID autogenerate-new-keypair save-to-keychain network-config testnet create

REGISTRATION_COST=$(near contract call-function as-read-only $VENEAR_ACCOUNT_ID storage_balance_bounds json-args {} network-config testnet now | jq '.min')

echo "Registration cost: $REGISTRATION_COST"
