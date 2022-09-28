#!/bin/bash

# Run with -h for documentation and help

# testnet deployment and values (default)
NEAR_ENV="testnet"
oracle_account_id="fpo.opfilabs.testnet"
provider_account_id="opfilabs.testnet"

while getopts ":a:ph" opt; do
  case $opt in
    h)
      echo "Builds and deploys the contract with state initialization (first deployement)."
      echo "Defaults to testnet."
      echo ""
      echo "Options:"
      echo "  -p              : for prod deployment"
      echo "  -a [account_id] : to overrirde \$ACCOUNT_ID"
      exit 0
      ;;
    p)
      NEAR_ENV="mainnet"
      oracle_account_id="fpo.opfilabs.near"
      provider_account_id="opfilabs.near"
      ;;
    a)
      ACCOUNT_ID="$OPTARG"
      ;;
    \?)
      echo "Invalid option -$OPTARG" >&2
      exit 1
      ;;
    :)
      echo "Option -$OPTARG needs a valid argument"
      exit 1
    ;;
  esac

done

printf "NEAR_ENV=%s\n" "$NEAR_ENV"
printf "ACCOUNT_ID=%s\n" "$ACCOUNT_ID"

./build.sh

near deploy -f --wasmFile ./out/conversion_proxy.wasm \
  --accountId $ACCOUNT_ID \
  --initFunction new  \
  --initArgs '{"oracle_account_id": "$oracle_account_id", "provider_account_id": "$provider_account_id"}'