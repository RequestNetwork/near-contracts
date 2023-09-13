#!/bin/bash
set -e

RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown
mkdir -p ./out
cp ./target/wasm32-unknown-unknown/debug/mocks.wasm ./out/
