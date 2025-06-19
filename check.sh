#!/bin/sh

set -e

echo Checking syntax...
cargo check # native
cargo check --target wasm32-unknown-unknown # web

echo Running tests...
cargo test --workspace --all-features

echo Running linter check...
cargo clippy --workspace --all-targets --all-features -- -D warnings # native
cargo clippy --workspace --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings # web

echo Running formatting check...
cargo fmt --all -- --check

echo Checking cargo doc...
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

echo All done!

# End of file
