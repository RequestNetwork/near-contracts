# Overview - temp

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

Run all contracts unit tests like this:

```
cd near-contracts/conversion_proxy
cargo test
cd near-contracts/fungible_conversion_proxy
cargo test
cd near-contracts/fungible_proxy
cargo test
cd near-contracts/mocks
cargo test
```

## Integration tests

```
# To test everything
./test.sh

# To test contracts one by one:
cargo test conversion_proxy
cargo test fungible_conversionproxy
cargo test fungible_proxy

# To run integration tests one by one (examples with main transfers):
cargo test conversion_proxy::test_transfer -- --exact
cargo test fungible_conversionproxy::test_transfer -- --exact
cargo test fungible_proxy::test_transfer -- --exact
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

Commands below assumes a few variables are set: `ACCOUNT_ID`, `BUILDER_ID` and `ISSUER_ID`.

This snippet makes a NEAR payment for $80.50, with a $1.00 fee.

```
near call $ACCOUNT_ID transfer_with_reference '{"to": "'$ISSUER_ID'", "payment_reference": "0x1230012300001234", "amount": "8050", "currency": "USD", "fee_amount": "100", "fee_address": "'$BUILDER_ID'"}' --accountId $ACCOUNT_ID --gas 300000000000000 --deposit 30
```

This snippet makes a fungible token payment, given that `fau.reqnetwork.testnet` is a fungible token address and the `fungible_proxy` contract is deployed at `pay.reqnetwork.testnet`.

```
near call fau.reqnetwork.testnet ft_transfer_call '{"receiver_id": "pay.reqnetwork.testnet", "amount": "2500000000000000000", "msg": "{\"fee_address\": \"'$BUILDER_ID'\", \"fee_amount\": \"1000000000000000000\", \"payment_reference\": \"abc7c8bb1234fd12\", \"to\": \"'$ISSUER_ID'\"}"}' --accountId $ACCOUNT_ID --depositYocto 1 --gas 300000000000000
```

## FAU tokens (testnet)

The FAU token at `fau.reqnetwork.testnet` has 18 decimals and a total supply of 1'000'000.00 FAU.
It is based on the example at https://github.com/near/near-sdk-rs/tree/master/examples/fungible-token, slightly updated and deployed using the commands:

```
./build.sh
near create-account fau.reqnetwork.testnet --masterAccount reqnetwork.testnet --initialBalance 8
near deploy -f --wasmFile ./res/fungible_token.wasm --accountId fau.reqnetwork.testnet --initFunction new_default_meta --initArgs '{"owner_id": "reqnetwork.testnet", "total_supply": "1000000000000000000000000"}'
```

Get some FAU:

```
# Register the account
near call fau.reqnetwork.testnet storage_deposit '{"account_id": "'$ACCOUNT_ID'"}' --accountId $ACCOUNT_ID --amount 0.005
# Transfer 1000.00 FAU to the account
near call fau.reqnetwork.testnet ft_transfer '{"receiver_id": "'$ACCOUNT_ID'", "amount": "1000000000000000000000"}' --accountId reqnetwork.testnet --depositYocto 1
```

To use FAU on a new proxy, you need to register it first:

```
 near call fau.reqnetwork.testnet storage_deposit '{"account_id": "'$PROXY_ADDRESS'"}' --accountId $ACCOUNT_ID --amount 0.005
```

You need to run the same command for every account before they receive FAU, or the smart contract will panick with the error message `The account some_account is not registered`.
