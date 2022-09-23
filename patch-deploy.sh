#!/bin/bash
near deploy -f --wasmFile ./out/conversion_proxy.wasm \
  --accountId $ACCOUNT_ID \