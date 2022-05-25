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
export ACCOUNT_ID=your_near_account
cd contracts
cargo
./build.sh
./deploy.sh
```
