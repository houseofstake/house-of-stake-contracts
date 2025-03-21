#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

: ${ROOT_ACCOUNT_ID:=$1}
SUFFIX=$2

# Fail if the root account ID is not set
if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Usage: $0 root_account_id. (or specify ROOT_ACCOUNT_ID in the environment)"
  exit 1
fi

: "${CHAIN_ID:=testnet}"
: "${VENEAR_ACCOUNT_ID:=v.$ROOT_ACCOUNT_ID}"

CURRENT_TIMESTAMP=$(date +%s)
export ACCOUNT_ID="acc-$CURRENT_TIMESTAMP$SUFFIX.$CHAIN_ID"

echo "Creating account $ACCOUNT_ID"
near --quiet account create-account sponsor-by-faucet-service $ACCOUNT_ID autogenerate-new-keypair save-to-keychain network-config $CHAIN_ID create

REGISTRATION_COST=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID storage_balance_bounds json-args {} network-config $CHAIN_ID now | jq '.min' | tr -d '"')

echo "Registration cost: $REGISTRATION_COST"

LOCKUP_COST=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_lockup_deployment_cost json-args {} network-config $CHAIN_ID now | jq . | tr -d '"')

echo "Lockup cost: $LOCKUP_COST"

# Wait for the account to be added to the keychain
sleep 2

echo "Registering the account"
ST=$(near --quiet contract call-function as-transaction $VENEAR_ACCOUNT_ID storage_deposit json-args '{}' prepaid-gas '10.0 Tgas' attached-deposit ''$REGISTRATION_COST' yoctoNEAR' sign-as $ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send)

echo "Deploying the lockup contract"
LOCKUP_ACCOUNT_ID=$(near --quiet contract call-function as-transaction $VENEAR_ACCOUNT_ID deploy_lockup json-args '{}' prepaid-gas '100.0 Tgas' attached-deposit ''$LOCKUP_COST' yoctoNEAR' sign-as $ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send | jq . | tr -d '"')
echo "Lockup account ID: $LOCKUP_ACCOUNT_ID"

export LOCKUP_ACCOUNT_ID=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_lockup_account_id json-args '{"account_id": "'$ACCOUNT_ID'"}' network-config $CHAIN_ID now | jq . | tr -d '"')

echo "Done"
echo "Account ID:        $ACCOUNT_ID"
echo "Lockup account ID: $LOCKUP_ACCOUNT_ID"
echo "Export commands:"
echo "export ACCOUNT_ID=$ACCOUNT_ID"

popd
