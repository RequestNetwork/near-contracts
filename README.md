# Quick Start

## Unit tests

```
cd contracts/conversion_proxy
cargo test
```

## Integration tests

First build the mock contract.

```
cd contracts/mocks
./build.sh
```

Then test.

```
cd contracts
cargo test
```

## Deploying contract

```
near login
# follow instructions to login with the account you will use below for deployment

cargo
./build.sh
export ACCOUNT_ID=your_near_account

# 1. For the first-time deployment
./deploy.sh

# 2. For subsequent contract updates
./patch-deploy.sh
```

## Calling contract

The snippet below makes a NEAR payment for $80.50, with a $1.00 fee.

```
# set ACCOUNT_ID, BUILDER_ID and ISSUER_ID
near call $ACCOUNT_ID transfer_with_reference '{"to": "'$ISSUER_ID'", "payment_reference": "0x1230012300001234", "amount": "8050", "currency": "USD", "fee_amount": "100", "fee_address": "'$BUILDER_ID'"}' --accountId $ACCOUNT_ID --gas 300000000000000 --deposit 30
```
