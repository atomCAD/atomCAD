#!/bin/sh

set -e

echo Checking syntax...
cargo check # native
for host in $HOSTS; do
    cargo check --target $host
done
RUSTFLAGS=--cfg=web_sys_unstable_apis cargo check --target wasm32-unknown-unknown

echo Running tests...
cargo test
cargo test --doc --all-features

echo Running linter check...
cargo clippy --workspace --all-targets --all-features -- -D warnings # native
for host in $HOSTS; do
    cargo clippy --workspace --target $host --all-targets --all-features -- -D warnings
done
RUSTFLAGS=--cfg=web_sys_unstable_apis cargo clippy --workspace --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings # web

echo Running formatting check...
cargo fmt --all -- --check

echo All done!

# End of File
