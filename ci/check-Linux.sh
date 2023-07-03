#!/bin/sh

set -e

HOSTS=$(cat << EOM
aarch64-linux-android
thumbv7neon-linux-androideabi
x86_64-linux-android
i686-linux-android
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
EOM
)

. ci/check-common.sh

# End of file
