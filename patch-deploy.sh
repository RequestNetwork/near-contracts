#!/bin/bash

# Run with -h for documentation and help

# testnet deployment and values (default)
NEAR_ENV="testnet"

while getopts ":a:ph" opt; do
  case $opt in
    h)
      echo "Builds and deploys the contract without state initialization (subsequent deployements)."
      echo "Defaults to testnet."
      echo ""
      echo "Options:"
      echo "  -p              : for prod deployment"
      echo "  -a [account_id] : to override \$ACCOUNT_ID"
      exit 0
      ;;
    p)
      NEAR_ENV="mainnet"
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

# TODO
near deploy -f --wasmFile ./out/fungible_conversion_proxy.wasm \
  --accountId $ACCOUNT_ID \