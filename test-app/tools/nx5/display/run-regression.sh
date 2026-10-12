#!/bin/sh
# Run on an already verified, exclusively owned NX5. No SDK is loaded here.
set -u
if [ "$#" -ne 3 ]; then
    echo "usage: $0 DISPLAY_BINARY OUTPUT_DIRECTORY pattern|mixed|fault|textures" >&2
    exit 2
fi
binary=$1
output=$2
mode=$3
framebuffer=${NX5_FRAMEBUFFER:-/dev/fb0}
touch=${NX5_TOUCH_DEVICE:-}
case "$mode" in
    pattern) set -- --pattern --observe=300 --seconds=20 ;;
    mixed) set -- --pattern --split --observe=30 --seconds=8 ;;
    fault) : "${touch:?set NX5_TOUCH_DEVICE for input fault injection}"; set -- --pattern --observe=600 --seconds=30 ;;
    textures) set -- --textures --seconds=60 ;;
    *) echo "unknown case: $mode" >&2; exit 2 ;;
esac
set -- "$@" "--framebuffer=$framebuffer"
if [ -n "$touch" ]; then set -- "$@" "--touch=$touch"; fi
major=$(stat -L -c %t "$framebuffer") || exit 2
minor=$(stat -L -c %T "$framebuffer") || exit 2
fb_sysfs=/sys/dev/char/$((0x$major)):$((0x$minor))
base=$output/$mode
test -x "$binary" && test -d "$output" || exit 2
if [ -e "$base.log" ] || [ -e "$base.rc" ]; then
    echo "refusing to overwrite $base evidence" >&2
    exit 2
fi
if [ "$mode" = fault ]; then
    test -f "$output/inject-read-error.so" || exit 2
    test ! -e "$output/fail-read" || exit 2
fi

snapshot() {
    cat /proc/umap/vo > "$1.vo"
    cat /proc/media-mem > "$1.mmz"
    cat "$fb_sysfs/virtual_size" > "$1.virtual-size"
    cat "$fb_sysfs/pan" > "$1.pan"
    devmem 0x34101090 32 > "$1.wbc"
}

run_app() {
    unset SLINT_MCP_PORT LD_PRELOAD NX5_REGRESSION_INPUT NX5_REGRESSION_TRIGGER
    # Preserve the caller's font and SDK/plugin search paths.
    export SLINT_BRIDGE_TRACE=1
    if [ "$mode" = fault ]; then
        export LD_PRELOAD=$output/inject-read-error.so
        export NX5_REGRESSION_INPUT="$(readlink -f "$touch")"
        export NX5_REGRESSION_TRIGGER=$output/fail-read
    fi
    exec "$binary" "$@"
}

sha256sum "$binary" > "$base.binary.sha256"
snapshot "$base.initial"
dmesg > "$base.kernel-before"
cat /proc/uptime > "$base.start"
run_app "$@" > "$base.log" 2>&1 &
pid=$!
printf '%s\n' "$pid" > "$base.pid"

# Prove that all three TDE allocations existed, rather than accepting an exit
# from a process that never initialized acceleration.
attempts=0
while [ "$attempts" -lt 100 ] && kill -0 "$pid" 2>/dev/null; do
    grep -q '^slint-bridge: TDE enabled;' "$base.log" && break
    sleep 0.1
    attempts=$((attempts + 1))
done
snapshot "$base.live"

if [ "$mode" = fault ]; then
    attempts=0
    while [ "$attempts" -lt 300 ] && kill -0 "$pid" 2>/dev/null; do
        grep -q '^frame=29 ' "$base.log" && break
        sleep 0.1
        attempts=$((attempts + 1))
    done
    if grep -q '^frame=29 ' "$base.log"; then
        snapshot "$base.before"
        # This firmware reads 0 at 0x34101090 even during capture. Require
        # real received frames and the driver's live WBC allocation instead.
        if grep -q 'name="vou-wbc"' "$base.before.mmz" &&
            awk '/-----WBC INFO/ { wbc=1; next }
                wbc && $1==0 && $2>0 && $3>0 && $4==420 && $6>0 && $11>=30 { active=1 }
                END { exit !active }' "$base.before.vo" &&
            ! grep -q '^display: observation result=' "$base.log"; then
            cat /proc/uptime > "$base.trigger"
            : > "$output/fail-read"
        else
            echo "WBC activity was not confirmed; input error was not injected" > "$base.injection-error"
        fi
    else
        echo "WBC did not reach frame 29; input error was not injected" > "$base.injection-error"
    fi
fi

wait "$pid"
result=$?
printf '%s\n' "$result" > "$base.rc"
cat /proc/uptime > "$base.end"
snapshot "$base"
dmesg > "$base.kernel"
# This marks snapshots complete, including when the expected fault exits 1.
: > "$base.done"
exit "$result"
