#!/bin/sh

set -e

HOSTS=$(cat << EOM
EOM
)

. ci/check-common.sh

# End of file
