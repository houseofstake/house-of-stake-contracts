#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

ACCOUNT_ID=$1
LOCKUP_ACCOUNT_ID=$2

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

if [ -z "$LOCKUP_ACCOUNT_ID" ]; then
  export LOCKUP_ACCOUNT_ID=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_lockup_account_id json-args '{"account_id": "'$ACCOUNT_ID'"}' network-config $CHAIN_ID now | tr -d '"')
fi

export LOCKED_BALANCE=$(near --quiet contract call-function as-read-only $LOCKUP_ACCOUNT_ID get_venear_locked_balance json-args '{}' network-config $CHAIN_ID now | tr -d '"')
export FT_BALANCE=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID ft_balance_of json-args '{"account_id": "'$ACCOUNT_ID'"}' network-config $CHAIN_ID now | tr -d '"')

LOCKED_BALANCE_NEAR=$(echo "scale=3; $LOCKED_BALANCE / 1000000000000000000000000" | bc)
FT_BALANCE_NEAR=$(echo "scale=3; $FT_BALANCE / 1000000000000000000000000" | bc)

echo "Account ID:     $ACCOUNT_ID"
echo "Locked balance: $LOCKED_BALANCE_NEAR NEAR"
echo "FT balance:     $FT_BALANCE_NEAR NEAR"
