#!/bin/sh
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_SDK_INCLUDE:?set the matching NX5 codec header directory}"
test_dir=$(mktemp -d /tmp/slint-bridge-tde-native.XXXXXX)
test_binary="$test_dir/test-native"
trap 'rm -rf "$test_dir"' EXIT
"${HOST_CC:-cc}" -std=gnu11 -O1 -g -Wall -Wextra -Werror \
    -fsanitize=address,undefined -I "$NX5_SDK_INCLUDE" \
    test-app/tools/nx5/tde/test-native.c -ldl -o "$test_binary"
# LeakSanitizer cannot run under the executor's tracing environment. The test
# asserts allocation release counts, including deliberate quarantine retention.
ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_leaks=0" "$test_binary"
"${HOST_CC:-cc}" -shared -fPIC -Wall -Wextra -Werror \
    -Wl,-soname,libVideoDec-compatible-rebuild.so \
    test-app/tools/nx5/tde/sdk-stub.c -o "$test_dir/libVideoDec-compatible-rebuild.so"
for directory in "$test_dir/first location" "$test_dir/relocated"; do
    mkdir "$directory"
    cp "$test_dir/libVideoDec-compatible-rebuild.so" "$directory/"
    ln -s libVideoDec-compatible-rebuild.so "$directory/libVideoDec.so"
    ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_leaks=0" \
        LD_LIBRARY_PATH="$directory" "$test_binary" --sdk-loading
done
