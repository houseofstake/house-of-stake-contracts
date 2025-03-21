#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

ACCOUNT_ID=$1

if [ -z "$ACCOUNT_ID" ]; then
  echo "Usage: $0 account_id."
  exit 1
fi

if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Please set the ROOT_ACCOUNT_ID in the environment."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
export VENEAR_ACCOUNT_ID="v.$ROOT_ACCOUNT_ID"

export LOCKUP_ACCOUNT_ID=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_lockup_account_id json-args '{"account_id": "'$ACCOUNT_ID'"}' network-config $CHAIN_ID now | tr -d '"')

near --quiet contract call-function as-transaction $LOCKUP_ACCOUNT_ID lock_near json-args '{}' prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Waiting for the lockup to complete"
sleep 3

scripts/view_balance.sh $ACCOUNT_ID $LOCKUP_ACCOUNT_ID
