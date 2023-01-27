# Overview

Smart contracts on NEAR used by the
[Request Network](https://github.com/RequestNetwork/requestNetwork) protocol.

## Setup

1. Install [Rust](https://www.rust-lang.org/tools/install)
2. Install GCC
   ```
   sudo apt install build-essential
   ```
3. Install clang
   ```
   sudo apt install clang
   ```
4. Install `wasm32-unknown-unknown` target
   ```
   rustup target add wasm32-unknown-unknown
   ```

## Unit tests

```
cd near-contracts/conversion_proxy
cargo test
cd near-contracts/fungible_conversion_proxy
cargo test
cd near-contracts/mocks
cargo test
```

## Integration tests

```
./test.sh
```

## Deploying contract

```
near login
# follow instructions to login with the account you will use below for deployment

cargo

# 1. For the first-time deployment
./deploy.sh -a ACCOUNT_ID

# 2. For subsequent contract updates
./patch-deploy.sh -a ACCOUNT_ID

# For both commands, use `-p` for production deployment.
```

## Calling contract

The snippet below makes a NEAR payment for $80.50, with a $1.00 fee.

```
# set ACCOUNT_ID, BUILDER_ID and ISSUER_ID
near call $ACCOUNT_ID transfer_with_reference '{"to": "'$ISSUER_ID'", "payment_reference": "0x1230012300001234", "amount": "8050", "currency": "USD", "fee_amount": "100", "fee_address": "'$BUILDER_ID'"}' --accountId $ACCOUNT_ID --gas 300000000000000 --deposit 30
```
