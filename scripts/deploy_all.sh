#!/usr/bin/env bash
set -e

pushd $(dirname $0)/..

ROOT_ACCOUNT_ID=$1

# Fail if the root account ID is not set
if [ -z "$ROOT_ACCOUNT_ID" ]; then
  echo "Usage: $0 root_account_id"
  echo "Please set the root account ID."
  exit 1
fi

# Fail if the root account ID is longer than 20 characters
if [ ${#ROOT_ACCOUNT_ID} -gt 20 ]; then
  echo "Error: The root account ID must be at most 20 characters long."
  exit 1
fi

: "${CHAIN_ID:=testnet}"
: "${STAKING_POOL_WHITELIST_ACCOUNT_ID:=whitelist.f863973.m0}"
# 10 minutes for testing
: "${UNLOCK_DURATION_SEC:=600}"
UNLOCK_DURATION_NS="${UNLOCK_DURATION_SEC}000000000"
# 0.1 NEAR (enough for 10000 bytes)
: ${LOCAL_DEPOSIT:="100000000000000000000000"}
# 2 NEAR
: ${MIN_LOCKUP_DEPOSIT:="2000000000000000000000000"}
# 10 minutes for testing
: "${VOTING_DURATION_SEC:=600}"
VOTING_DURATION_NS="${VOTING_DURATION_SEC}000000000"
# 0.1 NEAR
: ${BASE_PROPOSAL_FEE:="100000000000000000000000"}
# 0.00125 NEAR (we probably need less)
: ${VOTE_STORAGE_FEE:="1250000000000000000000"}

# Shorter name, so we can fit more
export ROOT_ACCOUNT_ID="$ROOT_ACCOUNT_ID"
export VENEAR_ACCOUNT_ID="v.$ROOT_ACCOUNT_ID"
export APPROVER_ACCOUNT_ID="approver.$ROOT_ACCOUNT_ID"
export VOTING_ACCOUNT_ID="vote.$ROOT_ACCOUNT_ID"
export OWNER_ACCOUNT_ID="owner.$ROOT_ACCOUNT_ID"
export LOCKUP_DEPLOYER_ACCOUNT_ID="lockup-deployer.$ROOT_ACCOUNT_ID"

echo "Creating account $VENEAR_ACCOUNT_ID"
near --quiet account create-account fund-myself $VENEAR_ACCOUNT_ID '2.3 NEAR' autogenerate-new-keypair save-to-keychain sign-as $ROOT_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Creating account $VOTING_ACCOUNT_ID"
near --quiet account create-account fund-myself $VOTING_ACCOUNT_ID '2.2 NEAR' autogenerate-new-keypair save-to-keychain sign-as $ROOT_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Creating account $OWNER_ACCOUNT_ID"
near --quiet account create-account fund-myself $OWNER_ACCOUNT_ID '0.1 NEAR' autogenerate-new-keypair save-to-keychain sign-as $ROOT_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Creating account $LOCKUP_DEPLOYER_ACCOUNT_ID"
near --quiet account create-account fund-myself $LOCKUP_DEPLOYER_ACCOUNT_ID '2.1 NEAR' autogenerate-new-keypair save-to-keychain sign-as $ROOT_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Deploying and initializing veNEAR contract"
near --quiet contract deploy $VENEAR_ACCOUNT_ID use-file res/local/venear_contract.wasm with-init-call new json-args '{
  "config": {
    "unlock_duration_ns": "'$UNLOCK_DURATION_NS'",
    "staking_pool_whitelist_account_id": "'$STAKING_POOL_WHITELIST_ACCOUNT_ID'",
    "lockup_code_deployers": ["'$LOCKUP_DEPLOYER_ACCOUNT_ID'"],
    "local_deposit": "'$LOCAL_DEPOSIT'",
    "min_lockup_deposit": "'$MIN_LOCKUP_DEPOSIT'",
    "owner_account_id": "'$OWNER_ACCOUNT_ID'"
  },
  "venear_growth_config": {
    "annual_growth_rate_ns": {
      "numerator": "6",
      "denominator": "31536000000000000"
    }
  }
}' prepaid-gas '10.0 Tgas' attached-deposit '0 NEAR' network-config $CHAIN_ID sign-with-keychain send

echo "Creating account $APPROVER_ACCOUNT_ID"
near --quiet account create-account fund-myself $APPROVER_ACCOUNT_ID '0.1 NEAR' autogenerate-new-keypair save-to-keychain sign-as $ROOT_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Deploying and initializing voting contract"
near --quiet contract deploy $VOTING_ACCOUNT_ID use-file res/local/voting_contract.wasm with-init-call new json-args '{
  "config": {
    "venear_account_id": "'$VENEAR_ACCOUNT_ID'",
    "approver_id": "'$APPROVER_ACCOUNT_ID'",
    "owner_account_id": "'$OWNER_ACCOUNT_ID'",
    "voting_duration_ns": "'$VOTING_DURATION_NS'",
    "max_number_of_voting_options": 16,
    "base_proposal_fee": "'$BASE_PROPOSAL_FEE'",
    "vote_storage_fee": "'$VOTE_STORAGE_FEE'"
  }
}' prepaid-gas '10.0 Tgas' attached-deposit '0 NEAR' network-config $CHAIN_ID sign-with-keychain send

echo "Preparing lockup contract on veNEAR"
near --quiet contract call-function as-transaction $VENEAR_ACCOUNT_ID prepare_lockup_code file-args res/local/lockup_contract.wasm prepaid-gas '100.0 Tgas' attached-deposit '1.98 NEAR' sign-as $LOCKUP_DEPLOYER_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

CONTRACT_HASH=$(cat res/local/lockup_contract.wasm | sha256sum | awk '{ print $1 }' | xxd -r -p | base58)
echo "Activating lockup contract on veNEAR with hash $CONTRACT_HASH"
near --quiet contract call-function as-transaction $VENEAR_ACCOUNT_ID set_lockup_contract json-args '{
  "contract_hash": "'$CONTRACT_HASH'",
  "min_lockup_deposit": "'$MIN_LOCKUP_DEPOSIT'"
}' prepaid-gas '20.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as $OWNER_ACCOUNT_ID network-config $CHAIN_ID sign-with-keychain send

echo "Done deploying!"
echo "Accounts:"
echo "veNEAR:            $VENEAR_ACCOUNT_ID"
echo "Voting:            $VOTING_ACCOUNT_ID"
echo "Owner:             $OWNER_ACCOUNT_ID"
echo "Lockup deployer:   $LOCKUP_DEPLOYER_ACCOUNT_ID"
echo "Proposal approver: $APPROVER_ACCOUNT_ID"
echo "Export commands:"
echo "export ROOT_ACCOUNT_ID=$ROOT_ACCOUNT_ID"
echo "export VENEAR_ACCOUNT_ID=$VENEAR_ACCOUNT_ID"
echo "export VOTING_ACCOUNT_ID=$VOTING_ACCOUNT_ID"
echo "export OWNER_ACCOUNT_ID=$OWNER_ACCOUNT_ID"
echo "export LOCKUP_DEPLOYER_ACCOUNT_ID=$LOCKUP_DEPLOYER_ACCOUNT_ID"
echo "export APPROVER_ACCOUNT_ID=$APPROVER_ACCOUNT_ID"

popd
