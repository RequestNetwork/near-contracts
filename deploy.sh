#!/bin/bash

# Run with -h for documentation and help

# testnet deployment and values (default)
NEAR_ENV="testnet";
oracle_account_id="fpo.opfilabs.testnet"
provider_account_id="opfilabs.testnet"
contract_name="conversion_proxy";
patch=false;

die() { echo "$*" >&2; exit 2; }  # complain to STDERR and exit with error
needs_arg() { if [ -z "$OPTARG" ]; then die "Missing arg for --$OPT option"; fi; }

while getopts "pha:-:" OPT; do
  # Source: https://stackoverflow.com/a/28466267
  if [ "$OPT" = "-" ]; then   # long option: reformulate OPT and OPTARG
    OPT="${OPTARG%%=*}"       # extract long option name
    OPTARG="${OPTARG#$OPT}"   # extract long option argument (may be empty)
    OPTARG="${OPTARG#=}"      # if long option argument, remove assigning `=`
  fi
  case "$OPT" in
    h | help)
      echo "Builds and deploys contracts, with or without state initialization."
      echo "Defaults to testnet."
      echo ""
      echo "Options:"
      echo "  -h | --help                 : shows this help"
      echo "  -p | --prod | --mainnet     : for prod deployment"
      echo "  -a [account_id]             : to override \$ACCOUNT_ID"
      echo "  --patch                     : to patch an existing contract (skip the init function, if any)"
      echo ""
      echo "  Choose the contract to deploy with:"
      echo "    --conversion_proxy [default]"
      echo "    --fungible_proxy"
      echo "    --fungible_conversionproxy"
      exit 0
      ;;
    # Options
    p | prod | mainnet) NEAR_ENV="mainnet" ;;
    a | account_id) needs_arg; ACCOUNT_ID="$OPTARG" ;;
    patch) patch=true ;;
    # Contract to deploy
    conversion_proxy | fungible_proxy | fungible_conversion_proxy) contract_name="$OPT" ;;
    # Bad options
    ??* )          die "Unknown option --$OPT" ;;   # bad long option
    ? )            exit 2 ;;                        # bad short option (error reported via getopts)
  esac
done

if [ "$ACCOUNT_ID" = "" ]; then
 echo "Missing account ID";
 exit 1;
fi

printf "Deploying %s on NEAR_ENV=%s with ACCOUNT_ID=%s (patch=%s)\n\n" "$contract_name" "$NEAR_ENV" "$ACCOUNT_ID" "$patch"


./build.sh

if [ "$contract_name" = "fungible_proxy" ]; then
  set -x
  near deploy -f --wasmFile ./target/wasm32-unknown-unknown/release/$contract_name.wasm \
   --accountId $ACCOUNT_ID
else
  
  if [ "$contract_name" = "conversion_proxy" ]; then
    if [ "$NEAR_ENV" = "mainnet" ]; then
      feed_parser="switchboard-v2.mainnet";
      feed_address="C3p8SSWQS8j1nx7HrzBBphX5jZcS1EY28EJ5iwjzSix2";
    else
      feed_parser="switchboard-v2.testnet";
      feed_address="7igqhpGQ8xPpyjQ4gMHhXRvtZcrKSGJkdKDJYBiPQgcb";
    fi
    initArgs='{"feed_parser":"'$feed_parser'","feed_address_pk":"'$feed_address'"}';
  else
    initArgs='{"oracle_account_id": "'$oracle_account_id'", "provider_account_id": "'$provider_account_id'"}';
  fi
  echo $initArgs;
  initParams="";
  if ! $patch ; then
    initParams="
    --initFunction new  \
    --initArgs $initArgs";
  fi
  set -x
  near deploy -f --wasmFile ./target/wasm32-unknown-unknown/release/$contract_name.wasm \
    --accountId $ACCOUNT_ID \
    $initParams
fi

set +x
