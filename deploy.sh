near deploy -f --wasmFile ./out/conversion_proxy.wasm \
  --accountId ACCOUNT_ID \
  --initFunction new  \
  --initArgs '{"oracle_account_id": "fpo.opfilabs.testnet", "provider_account_id": "opfilabs.testnet"}'