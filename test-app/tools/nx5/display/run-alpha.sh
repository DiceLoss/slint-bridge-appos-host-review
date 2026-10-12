#!/bin/sh
# Exclusive finite UI regression; caller supplies a directory with both binaries/plugin.
set -u
if [ "$#" -ne 2 ]; then echo "usage: $0 BINARY_DIRECTORY tde|software" >&2; exit 2; fi
base=$1
mode=$2
case "$mode" in tde|software) ;; *) exit 2 ;; esac
: "${SLINT_DEFAULT_FONT:?set a deployed font}"
: "${LD_LIBRARY_PATH:?set the SDK search path}"
: "${NX5_FRAMEBUFFER:=/dev/fb0}"
output="$base/$mode-results"
if [ -e "$output/alpha.log" ]; then echo "refusing to overwrite $output evidence" >&2; exit 2; fi
mkdir -p "$output"
unset SLINT_MCP_PORT LD_PRELOAD SLINT_BRIDGE_TRACE
cat /proc/media-mem > "$output/initial.mmz"
dmesg > "$output/kernel-before"
LD_LIBRARY_PATH="$base:$LD_LIBRARY_PATH" "$base/alpha-$mode" "$output" \
    "--framebuffer=$NX5_FRAMEBUFFER" > "$output/alpha.log" 2>&1 &
alpha_pid=$!
echo "$alpha_pid" > "$output/pid"
sleep 1
cat /proc/media-mem > "$output/live.mmz"
wait "$alpha_pid"
alpha_rc=$?
echo "$alpha_rc" > "$output/rc"
cat /proc/media-mem > "$output/final.mmz"
cat /proc/umap/vo > "$output/final.vo"
dmesg > "$output/kernel-after"
touch "$output/done"
cat "$output/alpha.log"
printf 'alpha_mode=%s exit=%s pid=%s\n' "$mode" "$alpha_rc" "$alpha_pid"
exit "$alpha_rc"
