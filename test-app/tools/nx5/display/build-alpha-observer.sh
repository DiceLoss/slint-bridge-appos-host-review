#!/bin/sh
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_CC:=/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc}"
: "${NX5_SDK_INCLUDE:?set the matching codec header directory}"
: "${NX5_SDK_LIBRARY:?set the compatible SDK library path}"
mkdir -p target/nx5-display
"$NX5_CC" -std=gnu11 -O2 -Wall -Wextra -Werror -fPIC -shared \
    -I "$NX5_SDK_INCLUDE" test-app/tools/nx5/display/alpha-observe.c \
    "$NX5_SDK_LIBRARY" -o target/nx5-display/libalpha-observer.so
