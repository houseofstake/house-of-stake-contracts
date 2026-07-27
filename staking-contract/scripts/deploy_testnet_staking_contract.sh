#!/usr/bin/env bash
#
# Deploy, upgrade, and configure staking-contract on NEAR testnet.
#
# Prerequisites:
#   - near CLI installed and keychain has the signer account(s).
#   - jq installed for deployment/config JSON.
#   - WASM built at res/local/staking_contract.wasm, or run with BUILD_WASM=1.
#
# Usage:
#   BUILD_WASM=1 ./staking-contract/scripts/deploy_testnet_staking_contract.sh <staking-account.testnet>
#   ACTION=upgrade ./staking-contract/scripts/deploy_testnet_staking_contract.sh <staking-account.testnet>
#   ACTION=configure ./staking-contract/scripts/deploy_testnet_staking_contract.sh hos-e2e-0601144939.testnet
#   ACTION=upgrade-and-configure BUILD_WASM=1 ./staking-contract/scripts/deploy_testnet_staking_contract.sh hos-e2e-0601144939.testnet
#
# Common environment:
#   CHAIN_ID=testnet
#   ACTION=deploy | upgrade | configure | deploy-and-configure | upgrade-and-configure
#                                      # default: deploy
#   STAKING_WASM=res/local/staking_contract.wasm
#   OWNER_ACCOUNT_ID=<owner.testnet>      # default: staking account
#   GUARDIANS_JSON='["guardian.testnet"]' # default: []
#   DRY_RUN=1                             # print commands without sending txs
#
# Optional account creation for fresh testnet subaccounts:
#   CREATE_ACCOUNT=1 PARENT_ACCOUNT_ID=<parent.testnet> FUND_STAKING_NEAR='8 NEAR' ...
#
# Validator and catalog configuration:
#   VALIDATORS_JSON='[
#     {"validator_id":"pool.testnet"},
#     {"validator_id":"mock-pool.hos.testnet","owner_account_id":"hos.testnet","deploy_mock_pool":true}
#   ]'
#   CATALOG_JSON='[
#     {
#       "validator_id":"pool.testnet",
#       "owner_account_id":"pool-owner.testnet",
#       "name":"NEAR AI Credits",
#       "description":"One-off NEAR AI credits",
#       "prices":[
#         {
#           "name":"NEAR AI Credits",
#           "description":"One-off NEAR AI credit",
#           "amount":"10000000000000000000000",
#           "price_type":"OneOff",
#           "billing_period":null,
#           "lock_factor_near_months":"0",
#           "metadata":null,
#           "set_default":true
#         }
#       ]
#     }
#   ]'
#   AGENT_SUBSCRIPTION_CATALOG=1
#   AGENT_SUBSCRIPTION_VALIDATOR_ID=pool.testnet
#   AGENT_SUBSCRIPTION_OWNER_ACCOUNT_ID=pool-owner.testnet
#   NEAR_AI_CREDITS_CATALOG=1
#   NEAR_AI_CREDITS_VALIDATOR_ID=pool.testnet
#   NEAR_AI_CREDITS_OWNER_ACCOUNT_ID=pool-owner.testnet
#
# Testnet config defaults are intentionally testing-friendly. Review every value before
# using this script for production-like deployment.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

usage() {
  sed -n '2,32p' "$0" >&2
}

STAKING_ACCOUNT_ID="${1:-${STAKING_ACCOUNT_ID:-}}"
if [[ -z "$STAKING_ACCOUNT_ID" ]]; then
  usage
  exit 1
fi

: "${CHAIN_ID:=testnet}"
: "${ACTION:=deploy}"
: "${STAKING_WASM:=$REPO_ROOT/res/local/staking_contract.wasm}"
: "${MOCK_POOL_WASM:=$REPO_ROOT/res/local/mock_staking_pool_contract.wasm}"
: "${OWNER_ACCOUNT_ID:=$STAKING_ACCOUNT_ID}"
: "${GUARDIANS_JSON:=[]}"
: "${VALIDATORS_JSON:=[]}"
: "${CATALOG_JSON:=[]}"
: "${PRODUCT_SCAN_LIMIT:=200}"

: "${AGENT_SUBSCRIPTION_CATALOG:=0}"
: "${AGENT_SUBSCRIPTION_PRODUCT_NAME:=NEAR AI Agents}"
: "${AGENT_SUBSCRIPTION_PRODUCT_DESCRIPTION:=Monthly agent hosting subscription tiers}"
: "${AGENT_SUBSCRIPTION_VALIDATOR_ID:=}"
: "${AGENT_SUBSCRIPTION_OWNER_ACCOUNT_ID:=${VALIDATOR_OWNER_ACCOUNT_ID:-$OWNER_ACCOUNT_ID}}"

: "${NEAR_AI_CREDITS_CATALOG:=0}"
: "${NEAR_AI_CREDITS_PRODUCT_NAME:=NEAR AI Credits}"
: "${NEAR_AI_CREDITS_PRODUCT_DESCRIPTION:=One-off NEAR AI credits for chat-api payments}"
: "${NEAR_AI_CREDITS_PRICE_NAME:=NEAR AI Credit}"
: "${NEAR_AI_CREDITS_PRICE_DESCRIPTION:=One NEAR AI credit}"
: "${NEAR_AI_CREDITS_PRICE_AMOUNT_YOCTO:=400000000000000000000000}"
: "${NEAR_AI_CREDITS_VALIDATOR_ID:=}"
: "${NEAR_AI_CREDITS_OWNER_ACCOUNT_ID:=${VALIDATOR_OWNER_ACCOUNT_ID:-$OWNER_ACCOUNT_ID}}"

: "${CREATE_ACCOUNT:=0}"
: "${PARENT_ACCOUNT_ID:=}"
: "${FUND_STAKING_NEAR:=8 NEAR}"
: "${FUND_VALIDATOR_NEAR:=2 NEAR}"

: "${MIN_LOCK_DURATION_NS:=1}"
: "${MAX_LOCK_DURATION_NS:=63072000000000000}"
: "${EPOCH_UNSTAKE_SETTLE_EPOCHS:=1}"
: "${MIN_STORAGE_DEPOSIT_YOCTO:=10000000000000000000000}"
: "${PER_LOCK_STORAGE_STAKE_YOCTO:=0}"
: "${PER_FARM_POSITION_STORAGE_STAKE_YOCTO:=0}"
: "${PER_PURCHASE_STORAGE_STAKE_YOCTO:=0}"
: "${MIN_LOCK_AMOUNT_YOCTO:=1000000000000000000000000}"

if [[ "$ACTION" != "deploy" && "$ACTION" != "upgrade" && "$ACTION" != "configure" && "$ACTION" != "deploy-and-configure" && "$ACTION" != "upgrade-and-configure" ]]; then
  echo "ACTION must be deploy, upgrade, configure, deploy-and-configure, or upgrade-and-configure; got: $ACTION" >&2
  exit 1
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 not found in PATH." >&2
    exit 1
  fi
}

require_command near
require_command jq

needs_deploy=0
needs_upgrade=0
needs_configure=0
case "$ACTION" in
  deploy)
    needs_deploy=1
    ;;
  upgrade)
    needs_upgrade=1
    ;;
  configure)
    needs_configure=1
    ;;
  deploy-and-configure)
    needs_deploy=1
    needs_configure=1
    ;;
  upgrade-and-configure)
    needs_upgrade=1
    needs_configure=1
    ;;
esac

if ! printf '%s' "$GUARDIANS_JSON" | jq -e 'type == "array"' >/dev/null; then
  echo "GUARDIANS_JSON must be a JSON array, got: $GUARDIANS_JSON" >&2
  exit 1
fi
if ! printf '%s' "$VALIDATORS_JSON" | jq -e 'type == "array"' >/dev/null; then
  echo "VALIDATORS_JSON must be a JSON array, got: $VALIDATORS_JSON" >&2
  exit 1
fi
if ! printf '%s' "$CATALOG_JSON" | jq -e 'type == "array"' >/dev/null; then
  echo "CATALOG_JSON must be a JSON array, got: $CATALOG_JSON" >&2
  exit 1
fi

build_agent_subscription_catalog_json() {
  local validator_id="$1"
  local owner_account_id="$2"

  jq -n \
    --arg validator "$validator_id" \
    --arg owner "$owner_account_id" \
    --arg product_name "$AGENT_SUBSCRIPTION_PRODUCT_NAME" \
    --arg product_description "$AGENT_SUBSCRIPTION_PRODUCT_DESCRIPTION" \
    '[
      {
        validator_id: $validator,
        owner_account_id: $owner,
        name: $product_name,
        description: $product_description,
        prices: [
          {
            name: "Starter",
            description: "1 agent; stake range [1, 10] NEAR",
            amount: "1000000000000000000000000",
            price_type: "Recurring",
            billing_period: "Monthly",
            lock_factor_near_months: "1000000000000000000000000",
            metadata: {
              max_amount: "10000000000000000000000000"
            },
            set_default: true
          },
          {
            name: "Basic",
            description: "2 agents; stake range [10, 40] NEAR",
            amount: "10000000000000000000000000",
            price_type: "Recurring",
            billing_period: "Monthly",
            lock_factor_near_months: "1000000000000000000000000",
            metadata: {
              max_amount: "40000000000000000000000000"
            },
            set_default: false
          },
          {
            name: "Pro",
            description: "5 agents; stake range [40, 400] NEAR",
            amount: "40000000000000000000000000",
            price_type: "Recurring",
            billing_period: "Monthly",
            lock_factor_near_months: "1000000000000000000000000",
            metadata: {
              max_amount: "400000000000000000000000000"
            },
            set_default: false
          }
        ]
      }
    ]'
}

build_near_ai_credits_catalog_json() {
  local validator_id="$1"
  local owner_account_id="$2"

  jq -n \
    --arg validator "$validator_id" \
    --arg owner "$owner_account_id" \
    --arg product_name "$NEAR_AI_CREDITS_PRODUCT_NAME" \
    --arg product_description "$NEAR_AI_CREDITS_PRODUCT_DESCRIPTION" \
    --arg price_name "$NEAR_AI_CREDITS_PRICE_NAME" \
    --arg price_description "$NEAR_AI_CREDITS_PRICE_DESCRIPTION" \
    --arg amount "$NEAR_AI_CREDITS_PRICE_AMOUNT_YOCTO" \
    '[
      {
        validator_id: $validator,
        owner_account_id: $owner,
        name: $product_name,
        description: $product_description,
        prices: [
          {
            name: $price_name,
            description: $price_description,
            amount: $amount,
            price_type: "OneOff",
            billing_period: null,
            lock_factor_near_months: "0",
            metadata: null,
            set_default: true
          }
        ]
      }
    ]'
}

if [[ "$AGENT_SUBSCRIPTION_CATALOG" == "1" ]]; then
  if [[ -z "$AGENT_SUBSCRIPTION_VALIDATOR_ID" ]]; then
    if [[ "$(jq 'length' <<<"$VALIDATORS_JSON")" == "1" ]]; then
      AGENT_SUBSCRIPTION_VALIDATOR_ID="$(jq -r '.[0].validator_id' <<<"$VALIDATORS_JSON")"
    else
      echo "AGENT_SUBSCRIPTION_CATALOG=1 requires AGENT_SUBSCRIPTION_VALIDATOR_ID, unless VALIDATORS_JSON has exactly one validator." >&2
      exit 1
    fi
  fi

  agent_subscription_catalog="$(
    build_agent_subscription_catalog_json \
      "$AGENT_SUBSCRIPTION_VALIDATOR_ID" \
      "$AGENT_SUBSCRIPTION_OWNER_ACCOUNT_ID"
  )"
  CATALOG_JSON="$(jq -c --argjson existing "$CATALOG_JSON" --argjson generated "$agent_subscription_catalog" '$existing + $generated' <<<"{}")"
fi

if [[ "$NEAR_AI_CREDITS_CATALOG" == "1" ]]; then
  if [[ -z "$NEAR_AI_CREDITS_VALIDATOR_ID" ]]; then
    if [[ "$(jq 'length' <<<"$VALIDATORS_JSON")" == "1" ]]; then
      NEAR_AI_CREDITS_VALIDATOR_ID="$(jq -r '.[0].validator_id' <<<"$VALIDATORS_JSON")"
    else
      echo "NEAR_AI_CREDITS_CATALOG=1 requires NEAR_AI_CREDITS_VALIDATOR_ID, unless VALIDATORS_JSON has exactly one validator." >&2
      exit 1
    fi
  fi

  near_ai_credits_catalog="$(
    build_near_ai_credits_catalog_json \
      "$NEAR_AI_CREDITS_VALIDATOR_ID" \
      "$NEAR_AI_CREDITS_OWNER_ACCOUNT_ID"
  )"
  CATALOG_JSON="$(jq -c --argjson existing "$CATALOG_JSON" --argjson generated "$near_ai_credits_catalog" '$existing + $generated' <<<"{}")"
fi

needs_mock_pool=0
if printf '%s' "$VALIDATORS_JSON" | jq -e 'any(.[]; .deploy_mock_pool == true)' >/dev/null; then
  needs_mock_pool=1
fi

if [[ "${BUILD_WASM:-0}" == "1" ]]; then
  "$REPO_ROOT/staking-contract/scripts/build_staking_wasm.sh"
  if [[ "$needs_mock_pool" == "1" ]]; then
    make mock-staking-pool-contract
  fi
fi

if [[ "$needs_deploy" == "1" || "$needs_upgrade" == "1" ]]; then
  if [[ ! -f "$STAKING_WASM" ]]; then
    echo "Missing staking WASM: $STAKING_WASM" >&2
    echo "Run: BUILD_WASM=1 $0 $STAKING_ACCOUNT_ID" >&2
    exit 1
  fi
fi
if [[ "$needs_mock_pool" == "1" && ! -f "$MOCK_POOL_WASM" ]]; then
  echo "Missing mock pool WASM: $MOCK_POOL_WASM" >&2
  echo "Run with BUILD_WASM=1 or build it with: make mock-staking-pool-contract" >&2
  exit 1
fi

if [[ "$needs_deploy" == "1" && ! -f "$STAKING_WASM" ]]; then
  echo "Missing staking WASM: $STAKING_WASM" >&2
  echo "Run: BUILD_WASM=1 $0 $STAKING_ACCOUNT_ID" >&2
  exit 1
fi

run() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  if [[ "${DRY_RUN:-0}" != "1" ]]; then
    "$@"
  fi
}

near_tx() {
  local signer="$1"
  local contract_id="$2"
  local method_name="$3"
  local args_json="$4"
  local gas="$5"
  local deposit="$6"

  run near --quiet contract call-function as-transaction "$contract_id" "$method_name" \
    json-args "$args_json" \
    prepaid-gas "$gas" attached-deposit "$deposit" \
    sign-as "$signer" network-config "$CHAIN_ID" sign-with-keychain send
}

view_json() {
  local contract_id="$1"
  local method_name="$2"
  local args_json="$3"

  near --quiet contract call-function as-read-only "$contract_id" "$method_name" \
    json-args "$args_json" network-config "$CHAIN_ID" now
}

wasm_hash() {
  if [[ ! -f "$STAKING_WASM" ]]; then
    echo "missing"
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$STAKING_WASM" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$STAKING_WASM" | awk '{print $1}'
  else
    echo "sha256 unavailable"
  fi
}

get_validator_json() {
  local validator_id="$1"
  view_json "$STAKING_ACCOUNT_ID" get_validator "$(jq -n --arg v "$validator_id" '{validator_id:$v}')"
}

find_product_id() {
  local validator_id="$1"
  local name="$2"
  view_json "$STAKING_ACCOUNT_ID" get_products "$(jq -n --argjson limit "$PRODUCT_SCAN_LIMIT" '{from_index:0, limit:$limit}')" \
    | jq -er --arg validator "$validator_id" --arg name "$name" \
      '[.[] | select(.validator_id == $validator and .name == $name and .status == "Active")] | last.product_id'
}

find_price_id() {
  local product_id="$1"
  local price_spec="$2"
  local name amount price_type billing lock_factor price_id price_json
  name="$(jq -r '.name' <<<"$price_spec")"
  amount="$(jq -r '.amount' <<<"$price_spec")"
  price_type="$(jq -r '.price_type // "OneOff"' <<<"$price_spec")"
  billing="$(jq -c '.billing_period // null' <<<"$price_spec")"
  lock_factor="$(jq -r '.lock_factor_near_months // "0"' <<<"$price_spec")"

  while IFS= read -r price_id; do
    [[ -z "$price_id" || "$price_id" == "null" ]] && continue
    price_json="$(view_json "$STAKING_ACCOUNT_ID" get_price "$(jq -n --arg p "$price_id" '{price_id:$p}')")"
    if jq -e \
      --arg name "$name" \
      --arg amount "$amount" \
      --arg price_type "$price_type" \
      --arg lock_factor "$lock_factor" \
      --argjson billing "$billing" \
      '.name == $name
        and .amount == $amount
        and .price_type == $price_type
        and .billing_period == $billing
        and .lock_factor_near_months == $lock_factor
        and .status == "Active"' \
      >/dev/null <<<"$price_json"; then
      printf '%s\n' "$price_id"
      return 0
    fi
  done < <(view_json "$STAKING_ACCOUNT_ID" get_product "$(jq -n --arg p "$product_id" '{product_id:$p}')" | jq -r '.price_ids[]?')

  return 1
}

configure_validators() {
  local row validator_id owner_account_id create_account deploy_mock_pool fund_near validator_json pool_init validator_arg

  if [[ "$(jq 'length' <<<"$VALIDATORS_JSON")" == "0" ]]; then
    echo "No validators configured (VALIDATORS_JSON=[])."
    return
  fi

  echo "== Configuring validators =="
  while IFS= read -r row; do
    validator_id="$(jq -er '.validator_id' <<<"$row")"
    owner_account_id="$(jq -r '.owner_account_id // empty' <<<"$row")"
    create_account="$(jq -r '.create_account // false' <<<"$row")"
    deploy_mock_pool="$(jq -r '.deploy_mock_pool // false' <<<"$row")"
    fund_near="$(jq -r --arg fallback "$FUND_VALIDATOR_NEAR" '.fund_near // $fallback' <<<"$row")"

    if [[ "$create_account" == "true" ]]; then
      if [[ -z "$PARENT_ACCOUNT_ID" ]]; then
        echo "Validator $validator_id has create_account=true, but PARENT_ACCOUNT_ID is not set." >&2
        exit 1
      fi
      echo "Creating validator account $validator_id ($fund_near)"
      run near --quiet account create-account fund-myself "$validator_id" "$fund_near" \
        autogenerate-new-keypair save-to-keychain sign-as "$PARENT_ACCOUNT_ID" \
        network-config "$CHAIN_ID" sign-with-keychain send
    fi

    if [[ "$deploy_mock_pool" == "true" ]]; then
      if [[ -z "$owner_account_id" ]]; then
        echo "Validator $validator_id has deploy_mock_pool=true, but owner_account_id is not set." >&2
        exit 1
      fi
      echo "Deploying mock staking pool $validator_id"
      pool_init="$(jq -n --arg owner "$owner_account_id" '{owner_id:$owner}')"
      run near --quiet contract deploy "$validator_id" use-file "$MOCK_POOL_WASM" \
        with-init-call new json-args "$pool_init" \
        prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' \
        network-config "$CHAIN_ID" sign-with-keychain send
    fi

    if [[ "${DRY_RUN:-0}" == "1" ]]; then
      validator_arg="$(jq -n --arg validator "$validator_id" '{validator_id:$validator}')"
      near_tx "$OWNER_ACCOUNT_ID" "$STAKING_ACCOUNT_ID" add_validator "$validator_arg" '50.0 Tgas' '1 yoctoNEAR'
      continue
    fi

    validator_json="$(get_validator_json "$validator_id")"
    if [[ "$validator_json" == "null" ]]; then
      validator_arg="$(jq -n --arg validator "$validator_id" '{validator_id:$validator}')"
      near_tx "$OWNER_ACCOUNT_ID" "$STAKING_ACCOUNT_ID" add_validator "$validator_arg" '50.0 Tgas' '1 yoctoNEAR'
    else
      echo "Validator already allowlisted: $validator_id"
    fi
  done < <(jq -c '.[]' <<<"$VALIDATORS_JSON")
  echo
}

configure_catalog() {
  local row validator_id owner_account_id product_id product_name product_description product_arg price_row price_id price_arg default_price_arg set_default

  if [[ "$(jq 'length' <<<"$CATALOG_JSON")" == "0" ]]; then
    echo "No catalog entries configured (CATALOG_JSON=[])."
    return
  fi

  echo "== Configuring catalog =="
  while IFS= read -r row; do
    validator_id="$(jq -er '.validator_id' <<<"$row")"
    owner_account_id="$(jq -r '.owner_account_id // empty' <<<"$row")"
    if [[ -z "$owner_account_id" ]]; then
      echo "Catalog entry for $validator_id requires owner_account_id to sign product/price calls." >&2
      exit 1
    fi

    product_id="$(jq -r '.product_id // empty' <<<"$row")"
    product_name="$(jq -er '.name // .product.name' <<<"$row")"
    product_description="$(jq -r '.description // .product.description // ""' <<<"$row")"

    if [[ "${DRY_RUN:-0}" == "1" ]]; then
      product_arg="$(jq -n --arg validator "$validator_id" --arg name "$product_name" --arg description "$product_description" \
        '{validator_id:$validator, name:$name, description:$description}')"
      if [[ -z "$product_id" ]]; then
        near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" create_product "$product_arg" '200.0 Tgas' '1 yoctoNEAR'
        product_id="<product_id returned by create_product>"
      fi
    else
      if [[ -z "$product_id" ]]; then
        if product_id="$(find_product_id "$validator_id" "$product_name" 2>/dev/null)"; then
          echo "Product already exists: $product_name ($product_id)"
        else
          product_arg="$(jq -n --arg validator "$validator_id" --arg name "$product_name" --arg description "$product_description" \
            '{validator_id:$validator, name:$name, description:$description}')"
          near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" create_product "$product_arg" '200.0 Tgas' '1 yoctoNEAR'
          product_id="$(find_product_id "$validator_id" "$product_name")"
        fi
      fi
    fi

    while IFS= read -r price_row; do
      set_default="$(jq -r '.set_default // false' <<<"$price_row")"
      if [[ "${DRY_RUN:-0}" == "1" ]]; then
        price_id="$(jq -r '.price_id // empty' <<<"$price_row")"
        price_arg="$(jq -c --arg product_id "$product_id" '
          {
            product_id: $product_id,
            name: .name,
            description: (.description // ""),
            amount: .amount,
            price_type: (.price_type // "OneOff"),
            billing_period: (.billing_period // null),
            lock_factor_near_months: (.lock_factor_near_months // "0"),
            metadata: (.metadata // null)
          }' <<<"$price_row")"
        near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" create_price "$price_arg" '200.0 Tgas' '1 yoctoNEAR'
        if [[ -z "$price_id" ]]; then
          price_id="<price_id returned by create_price>"
        fi
        if [[ "$set_default" == "true" ]]; then
          default_price_arg="$(jq -n --arg product_id "$product_id" --arg price_id "$price_id" '{product_id:$product_id, price_id:$price_id}')"
          near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" set_product_default_price "$default_price_arg" '200.0 Tgas' '1 yoctoNEAR'
        fi
        continue
      fi

      if price_id="$(find_price_id "$product_id" "$price_row" 2>/dev/null)"; then
        echo "Price already exists: $(jq -r '.name' <<<"$price_row") ($price_id)"
      else
        price_arg="$(jq -c --arg product_id "$product_id" '
          {
            product_id: $product_id,
            name: .name,
            description: (.description // ""),
            amount: .amount,
            price_type: (.price_type // "OneOff"),
            billing_period: (.billing_period // null),
            lock_factor_near_months: (.lock_factor_near_months // "0"),
            metadata: (.metadata // null)
          }' <<<"$price_row")"
        near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" create_price "$price_arg" '200.0 Tgas' '1 yoctoNEAR'
        price_id="$(find_price_id "$product_id" "$price_row")"
      fi

      if [[ "$set_default" == "true" ]]; then
        default_price_arg="$(jq -n --arg product_id "$product_id" --arg price_id "$price_id" '{product_id:$product_id, price_id:$price_id}')"
        near_tx "$owner_account_id" "$STAKING_ACCOUNT_ID" set_product_default_price "$default_price_arg" '200.0 Tgas' '1 yoctoNEAR'
      fi
    done < <(jq -c '.prices // [] | .[]' <<<"$row")
  done < <(jq -c '.[]' <<<"$CATALOG_JSON")
  echo
}

echo "== House of Stake staking-contract testnet deploy =="
echo "Action:             $ACTION"
echo "Network:            $CHAIN_ID"
echo "Staking account:    $STAKING_ACCOUNT_ID"
echo "Owner account:      $OWNER_ACCOUNT_ID"
echo "WASM:               $STAKING_WASM"
echo "WASM sha256:        $(wasm_hash)"
echo "Validators:         $(jq 'length' <<<"$VALIDATORS_JSON")"
echo "Catalog products:   $(jq 'length' <<<"$CATALOG_JSON")"
echo

if [[ "$CHAIN_ID" != "testnet" ]]; then
  echo "Refusing to run: this script is for testnet only. Set CHAIN_ID=testnet." >&2
  exit 1
fi

if [[ "$CREATE_ACCOUNT" == "1" ]]; then
  if [[ -z "$PARENT_ACCOUNT_ID" ]]; then
    echo "CREATE_ACCOUNT=1 requires PARENT_ACCOUNT_ID=<parent.testnet>." >&2
    exit 1
  fi
  echo "== Creating staking account =="
  run near --quiet account create-account fund-myself "$STAKING_ACCOUNT_ID" "$FUND_STAKING_NEAR" \
    autogenerate-new-keypair save-to-keychain sign-as "$PARENT_ACCOUNT_ID" \
    network-config "$CHAIN_ID" sign-with-keychain send
  echo
fi

if [[ "$needs_deploy" == "1" ]]; then
  staking_init=$(
    jq -n \
      --arg owner "$OWNER_ACCOUNT_ID" \
      --argjson guardians "$GUARDIANS_JSON" \
      --arg min_lock_d "$MIN_LOCK_DURATION_NS" \
      --arg max_lock_d "$MAX_LOCK_DURATION_NS" \
      --argjson epoch_unstake "$EPOCH_UNSTAKE_SETTLE_EPOCHS" \
      --arg min_storage "$MIN_STORAGE_DEPOSIT_YOCTO" \
      --arg per_lock "$PER_LOCK_STORAGE_STAKE_YOCTO" \
      --arg per_farm_position "$PER_FARM_POSITION_STORAGE_STAKE_YOCTO" \
      --arg per_purchase "$PER_PURCHASE_STORAGE_STAKE_YOCTO" \
      --arg min_lock_amt "$MIN_LOCK_AMOUNT_YOCTO" \
      '{
        config: {
          owner_account_id: $owner,
          proposed_new_owner_account_id: null,
          guardians: $guardians,
          min_lock_duration_ns: $min_lock_d,
          max_lock_duration_ns: $max_lock_d,
          epoch_unstake_settle_epochs: $epoch_unstake,
          min_storage_deposit: $min_storage,
          per_lock_storage_stake: $per_lock,
          per_farm_position_storage_stake: $per_farm_position,
          per_purchase_storage_stake: $per_purchase,
          min_lock_amount: $min_lock_amt
        }
      }'
  )

  echo "== Deploying and initializing staking-contract =="
  echo "$staking_init" | jq .
  run near --quiet contract deploy "$STAKING_ACCOUNT_ID" use-file "$STAKING_WASM" \
    with-init-call new json-args "$staking_init" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    network-config "$CHAIN_ID" sign-with-keychain send
fi

if [[ "$needs_upgrade" == "1" ]]; then
  echo "== Upgrading staking-contract through owner-gated upgrade() =="
  echo "The owner account must match get_config.owner_account_id on-chain."
  run near --quiet contract call-function as-transaction "$STAKING_ACCOUNT_ID" upgrade \
    file-args "$STAKING_WASM" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    sign-as "$OWNER_ACCOUNT_ID" network-config "$CHAIN_ID" sign-with-keychain send
fi

if [[ "$needs_configure" == "1" ]]; then
  configure_validators
  configure_catalog
fi

echo
echo "== Post-deploy checks =="
run near contract call-function as-read-only "$STAKING_ACCOUNT_ID" get_version \
  json-args '{}' network-config "$CHAIN_ID" now
run near contract call-function as-read-only "$STAKING_ACCOUNT_ID" get_config \
  json-args '{}' network-config "$CHAIN_ID" now

echo
echo "Done."
echo "Useful next steps:"
echo "  Register buyers with storage_deposit before lock/pay flows."
echo "  Run ACTION=configure with VALIDATORS_JSON and CATALOG_JSON to add more validators/products/prices."
