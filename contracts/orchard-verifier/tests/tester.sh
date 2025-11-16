#! /bin/bash
export RUST_LOG=debug
export NEAR_SANDBOX_BIN_PATH=~/nearcore/target/debug/neard-sandbox

cargo test --package orchard-verifier --test gas_parse_build --all-features -- gas_parse_build --exact --nocapture
cargo test --package orchard-verifier --test gas_verify --all-features -- gas_verify --exact --nocapture