#!/bin/bash

# Run with -h for documentation and help

# testnet deployment and values (default)
NEAR_ENV="testnet"
oracle_account_id="fpo.opfilabs.testnet"
provider_account_id="opfilabs.testnet"
contract_name="conversion_proxy";

die() { echo "$*" >&2; exit 2; }  # complain to STDERR and exit with error
needs_arg() { if [ -z "$OPTARG" ]; then die "Missing arg for --$OPT option"; fi; }

while getopts "pha:-:" OPT; do
  if [ "$OPT" = "-" ]; then   # long option: reformulate OPT and OPTARG
    OPT="${OPTARG%%=*}"       # extract long option name
    OPTARG="${OPTARG#$OPT}"   # extract long option argument (may be empty)
    OPTARG="${OPTARG#=}"      # if long option argument, remove assigning `=`
  fi
  case "$OPT" in
    h | help)
      echo "Builds and deploys the contract with state initialization (first deployement)."
      echo "Defaults to testnet."
      echo ""
      echo "Options:"
      echo "  -p | --mainnet      : for prod deployment"
      echo "  -a [account_id]     : to override \$ACCOUNT_ID"
      echo "  Choose the contract to deploy with:"
      echo "    --conversion_proxy [default]"
      echo "    --fungible_proxy"
      echo "    --fungible_conversionproxy"
      exit 0
      ;;
    p | prod | mainnet) NEAR_ENV="mainnet" ;;
    a | account_id) needs_arg; ACCOUNT_ID="$OPTARG" ;;
    conversion_proxy | fungible_proxy | fungible_conversion_proxy) contract_name="$OPT" ;;
    ??* )          die "Unknown option --$OPT" ;;   # bad long option
    ? )            exit 2 ;;                        # bad short option (error reported via getopts)
  esac
done

if [ "$ACCOUNT_ID" = "" ]; then
 echo "Missing account ID";
 exit 1;
fi

printf "Deploying %s on NEAR_ENV=%s with ACCOUNT_ID=%s\n\n" "$contract_name" "$NEAR_ENV" "$ACCOUNT_ID"

./build.sh

if [ "$contract_name" = "fungible_proxy" ]; then
  near deploy -f --wasmFile ./out/$contract_name.wasm \
    --accountId $ACCOUNT_ID
else
  near deploy -f --wasmFile ./out/$contract_name.wasm \
    --accountId $ACCOUNT_ID \
    --initFunction new  \
    --initArgs '{"oracle_account_id": "'$oracle_account_id'", "provider_account_id": "'$provider_account_id'"}'
fi