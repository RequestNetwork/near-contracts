#!/bin/bash
set -e

RUSTFLAGS='-C link-arg=-s' cargo build -p mocks --target wasm32-unknown-unknown
