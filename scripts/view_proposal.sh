#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

PROPOSAL_ID=$1

if [ -z "$PROPOSAL_ID" ]; then
  echo "Usage: $0 proposal_id."
  exit 1
fi

if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Please set the ROOT_ACCOUNT_ID in the environment."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
export VOTING_ACCOUNT_ID="vote.$ROOT_ACCOUNT_ID"

export PROPOSAL=$(near --quiet contract call-function as-read-only $VOTING_ACCOUNT_ID get_proposal json-args '{"proposal_id": '$PROPOSAL_ID'}' network-config $CHAIN_ID now)
echo "Proposal"
echo $PROPOSAL | jq .
