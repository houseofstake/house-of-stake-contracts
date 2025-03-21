#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

: "${CHAIN_ID:=testnet}"

CURRENT_TIMESTAMP=$(date +%s)
ROOT_ACCOUNT_ID="r-$CURRENT_TIMESTAMP.$CHAIN_ID"
echo "Creating root account: $ROOT_ACCOUNT_ID"
near account create-account sponsor-by-faucet-service $ROOT_ACCOUNT_ID autogenerate-new-keypair save-to-keychain network-config testnet create

echo "Sleeping for 5 seconds"
sleep 5

. scripts/deploy_all.sh $ROOT_ACCOUNT_ID

echo "Creating 3 user accounts"

. scripts/create_account.sh $ROOT_ACCOUNT_ID "-user1"
export ACCOUNT_ID1=$ACCOUNT_ID

. scripts/create_account.sh $ROOT_ACCOUNT_ID "-user2"
export ACCOUNT_ID2=$ACCOUNT_ID

. scripts/create_account.sh $ROOT_ACCOUNT_ID "-user3"
export ACCOUNT_ID3=$ACCOUNT_ID

echo "Locking NEAR for user #1: $ACCOUNT_ID1"
scripts/lock_near.sh $ACCOUNT_ID1

echo "Locking NEAR for user #2: $ACCOUNT_ID2"
scripts/lock_near.sh $ACCOUNT_ID2

echo "Delegate all from user #1 to user #3: $ACCOUNT_ID1 to $ACCOUNT_ID3"
scripts/delegate.sh $ACCOUNT_ID1 $ACCOUNT_ID3

echo "Here are the balances (sorted)"
scripts/print_accounts.sh

sleep 5

echo "Creating proposal by user #1: $ACCOUNT_ID1"
. scripts/create_proposal.sh $ACCOUNT_ID1

echo "Approving proposal: $PROPOSAL_ID"
scripts/approve_proposal.sh $PROPOSAL_ID

sleep 2

echo "Voting by user #2 with vote #0: $ACCOUNT_ID2"
scripts/vote.sh $ACCOUNT_ID2 $PROPOSAL_ID 0

echo "Voting by user #3 with vote #1: $ACCOUNT_ID3"
scripts/vote.sh $ACCOUNT_ID3 $PROPOSAL_ID 1

sleep 2
scripts/view_proposal.sh $PROPOSAL_ID
