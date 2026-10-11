#!/bin/sh
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_CC:=/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc}"
mkdir -p target/nx5-present
"$NX5_CC" -std=c11 -O2 -Wall -Wextra -Werror -pthread \
    test-app/tools/nx5/present/probe.c -o target/nx5-present/probe
"$NX5_CC" -std=c11 -O2 -Wall -Wextra -Werror \
    test-app/tools/nx5/present/pattern.c -o target/nx5-present/pattern
if [ -n "${NX5_SDK_INCLUDE:-}" ] && [ -n "${NX5_SDK_LIB:-}" ]; then
    for name in wbc-query wbc-capture retire; do
        if [ "$name" = retire ]; then set -- -pthread; else set --; fi
        "$NX5_CC" -std=gnu11 -O2 -Wall -Wextra -Werror "$@" \
            -I "$NX5_SDK_INCLUDE" "test-app/tools/nx5/present/$name.c" \
            -L "$NX5_SDK_LIB" -lVideoDec -o "target/nx5-present/$name"
    done
fi
