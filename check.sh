#!/bin/sh

set -e

HOSTS=$(cat << EOM
aarch64-linux-android
thumbv7neon-linux-androideabi
x86_64-linux-android
i686-linux-android
aarch64-apple-ios
aarch64-apple-ios-sim
x86_64-apple-ios
aarch64-apple-darwin
x86_64-apple-darwin
x86_64-unknown-linux-gnu
i686-unknown-linux-gnu
riscv64gc-unknown-linux-gnu
aarch64-unknown-linux-gnu
thumbv7neon-unknown-linux-gnueabihf
powerpc64-unknown-linux-gnu
powerpc64le-unknown-linux-gnu
x86_64-pc-windows-msvc
x86_64-pc-windows-gnu
i686-pc-windows-msvc
i686-pc-windows-gnu
aarch64-pc-windows-msvc
aarch64-pc-windows-msvc
EOM
)

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

# End of file
