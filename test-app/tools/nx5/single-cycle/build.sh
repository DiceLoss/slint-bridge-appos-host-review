#!/bin/sh
set -eu
cd "$(dirname "$0")/../../../.."
: "${NX5_CC:=/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc}"
mkdir -p target/nx5-single-cycle
"$NX5_CC" -std=gnu11 -O2 -Wall -Wextra -Werror -pthread \
    test-app/tools/nx5/single-cycle/probe.c -o target/nx5-single-cycle/probe
