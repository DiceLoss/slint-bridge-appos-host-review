#!/bin/sh
# Run after identity/ownership checks. The application alone owns the SDK.
set -u
if [ "$#" -ne 3 ]; then
    echo "usage: $0 STARTUP_BINARY OUTPUT_ROOT show-error|retry|queued|timer|queued-retry" >&2
    exit 2
fi
binary=$1
root=$2
mode=$3
case "$mode" in show-error|retry|queued|timer|queued-retry) ;; *) exit 2 ;; esac
test -x "$binary" && test -d "$root" || exit 2
base=$root/$mode
test ! -e "$base.log" && test ! -e "$base.state" || exit 2
mkdir "$base.state" || exit 2
cat /proc/media-mem > "$base.initial.mmz"
cat /proc/umap/vo > "$base.initial.vo"
dmesg | tail -n 200 > "$base.kernel-before"
sha256sum "$binary" > "$base.binary.sha256"
cat /proc/uptime > "$base.start"
run_app() {
    unset SLINT_MCP_PORT LD_PRELOAD NX5_REGRESSION_INPUT NX5_REGRESSION_TRIGGER
    # Preserve the caller's SDK search path.
    export SLINT_BRIDGE_TRACE=1
    set -- "$mode" "$base.state" "--framebuffer=${NX5_FRAMEBUFFER:-/dev/fb0}"
    if [ -n "${NX5_TOUCH_DEVICE:-}" ]; then set -- "$@" "--touch=$NX5_TOUCH_DEVICE"; fi
    exec "$binary" "$@"
}
run_app > "$base.log" 2>&1 &
pid=$!
printf '%s\n' "$pid" > "$base.pid"
wait "$pid"
result=$?
printf '%s\n' "$result" > "$base.rc"
cat /proc/uptime > "$base.end"
cat /proc/media-mem > "$base.mmz"
cat /proc/umap/vo > "$base.vo"
dmesg | tail -n 200 > "$base.kernel"
: > "$base.done"
exit "$result"
