#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Please set the ROOT_ACCOUNT_ID in the environment."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
export VENEAR_ACCOUNT_ID="v.$ROOT_ACCOUNT_ID"

DATA=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_accounts json-args '{}' network-config $CHAIN_ID now)

export BALANCES=$(echo $DATA | jq 'map({
  account_id: .account.account_id,
  venear: (((.account.delegated_balance.extra_venear_balance | tonumber) +
   (.account.delegated_balance.near_balance | tonumber) + (
    if .account.delegation == null then
      (.account.balance.extra_venear_balance | tonumber) +
      (.account.balance.near_balance | tonumber)
    else
      0
    end
  )) / 1e24)
}) | sort_by(.venear) | reverse')

echo $BALANCES | jq .
