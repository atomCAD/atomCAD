#!/bin/sh

set -e

HOSTS=$(cat << EOM
aarch64-apple-ios
aarch64-apple-ios-sim
x86_64-apple-ios
aarch64-apple-darwin
x86_64-apple-darwin
EOM
)

. ci/check-common.sh

# End of file
