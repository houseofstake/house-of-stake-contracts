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
export VOTING_ACCOUNT_ID="vote.$ROOT_ACCOUNT_ID"

echo "Creating proposal"
export PROPOSAL_ID=$(near --quiet contract call-function as-transaction $VOTING_ACCOUNT_ID create_proposal json-args '{"metadata": {
  "title": "Test",
  "description": "Test Desc",
  "link": "https://example.com",
  "voting_options": ["A", "B"]
}}' prepaid-gas '100.0 Tgas' attached-deposit '0.2 NEAR' sign-as $ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send)

sleep 1

scripts/view_proposal.sh $PROPOSAL_ID
