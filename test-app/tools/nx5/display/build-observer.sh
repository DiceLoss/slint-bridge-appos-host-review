#!/bin/sh
# Build a plugin loaded by test-app/src/main.rs, never a second SDK process.
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_CC:=/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc}"
: "${NX5_SDK_INCLUDE:=$PWD/third-party/nx5/include}"
: "${NX5_SDK_LIBRARY:=$PWD/third-party/nx5/lib/libVideoDec.so}"
mkdir -p target/nx5-display
"$NX5_CC" -std=gnu11 -O2 -Wall -Wextra -Werror -fPIC -DBRIDGE_IN_PROCESS \
    -Dmain=bridge_wbc_observe -I "$NX5_SDK_INCLUDE" \
    -c test-app/tools/nx5/display/wbc-observe.c -o target/nx5-display/observe.o
"$NX5_CC" -shared target/nx5-display/observe.o \
    "$NX5_SDK_LIBRARY" -o target/nx5-display/libobserver.so
