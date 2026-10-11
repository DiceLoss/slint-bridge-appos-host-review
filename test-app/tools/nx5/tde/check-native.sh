#!/bin/sh
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_SDK_INCLUDE:?set the matching NX5 codec header directory}"
test_binary=$(mktemp /tmp/slint-bridge-tde-native.XXXXXX)
trap 'rm -f "$test_binary"' EXIT
"${HOST_CC:-cc}" -std=gnu11 -O1 -g -Wall -Wextra -Werror \
    -fsanitize=address,undefined -I "$NX5_SDK_INCLUDE" \
    test-app/tools/nx5/tde/test-native.c -ldl -o "$test_binary"
# LeakSanitizer cannot run under the executor's tracing environment. The test
# asserts allocation release counts, including deliberate quarantine retention.
ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_leaks=0" "$test_binary"
