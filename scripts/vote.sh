#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

ACCOUNT_ID=$1
PROPOSAL_ID=$2
VOTE=$3

if [ -z "$ACCOUNT_ID" ] || [ -z "$PROPOSAL_ID" ] || [ -z "$VOTE" ]; then
  echo "Usage: $0 account_id proposal_id vote."
  exit 1
fi

if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Please set the ROOT_ACCOUNT_ID in the environment."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
export VENEAR_ACCOUNT_ID="v.$ROOT_ACCOUNT_ID"
export VOTING_ACCOUNT_ID="vote.$ROOT_ACCOUNT_ID"

export PROPOSAL=$(near --quiet contract call-function as-read-only $VOTING_ACCOUNT_ID get_proposal json-args '{"proposal_id": '$PROPOSAL_ID'}' network-config $CHAIN_ID now)

SNAPSHOT_BLOCK_HEIGHT="$(echo $PROPOSAL | jq '.snapshot_and_state.snapshot.block_height')"
echo "Snapshot block height: $SNAPSHOT_BLOCK_HEIGHT"

export PROOF=$(near --quiet contract call-function as-read-only $VENEAR_ACCOUNT_ID get_proof json-args '{"account_id": "'$ACCOUNT_ID'"}' network-config $CHAIN_ID at-block-height $SNAPSHOT_BLOCK_HEIGHT)
echo "Got Proof. Voting"

MERKLE_PROOF=$(echo $PROOF | jq ".[0]")
V_ACCOUNT=$(echo $PROOF | jq ".[1]")
VOTE_ARGS=$(echo '{
  "proposal_id": '$PROPOSAL_ID',
  "vote": '$VOTE',
  "merkle_proof": '$MERKLE_PROOF',
  "v_account": '$V_ACCOUNT'
}' | jq -c .)

TMP=$(near --quiet contract call-function as-transaction $VOTING_ACCOUNT_ID vote json-args ''$VOTE_ARGS'' prepaid-gas '20.0 Tgas' attached-deposit '0.00125 NEAR' sign-as $ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send)

scripts/view_proposal.sh $PROPOSAL_ID
