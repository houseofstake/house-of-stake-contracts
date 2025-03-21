#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

FROM_ACCOUNT_ID=$1
TO_ACCOUNT_ID=$2

if [ -z "$FROM_ACCOUNT_ID" ] || [ -z "$TO_ACCOUNT_ID" ]; then
  echo "Usage: $0 FROM_ACCOUNT_ID TO_ACCOUNT_ID."
  exit 1
fi

if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Please set the ROOT_ACCOUNT_ID in the environment."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
export VENEAR_ACCOUNT_ID="v.$ROOT_ACCOUNT_ID"

TMP=$(near --quiet contract call-function as-transaction $VENEAR_ACCOUNT_ID delegate_all json-args '{"receiver_id": "'$TO_ACCOUNT_ID'"}' prepaid-gas '20.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $FROM_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send)

. scripts/view_balance.sh $TO_ACCOUNT_ID
TO_LOCKED_BALANCE_NEAR=$(echo "scale=3; $LOCKED_BALANCE / 1000000000000000000000000" | bc)
TO_FT_BALANCE_NEAR=$(echo "scale=3; $FT_BALANCE / 1000000000000000000000000" | bc)

. scripts/view_balance.sh $FROM_ACCOUNT_ID

echo "TO Account ID:  $TO_ACCOUNT_ID"
echo "Locked balance: $TO_LOCKED_BALANCE_NEAR NEAR"
echo "FT balance:     $TO_FT_BALANCE_NEAR NEAR"
