#!/bin/sh
# Run from usbtty1 as root. Pause, never terminate, the current UI owner.
set -eu
cd "$(dirname "$0")"
owner=${1:?usage: run-device.sh UI_PID SAMPLE_COUNT RUN_NAME}
count=${2:?sample count required}
run=${3:?run name required}
mode=${4:-flips}
case "$mode" in flips | white | split | retire | early-reuse) ;; *) exit 2 ;; esac
stress=${NX5_CPU_STRESS:-0}
case "$stress" in 0 | 2) ;; *) exit 2 ;; esac
case "$owner:$count" in *[!0-9:]* | :* | *:) exit 2 ;; esac
case "$run" in *[!a-zA-Z0-9_-]* | '') exit 2 ;; esac
[ ! -e "$run.log" ] || { echo "refusing to overwrite an existing run"; exit 2; }
[ "$(id -u)" = 0 ] || { echo "requires root for documented MMIO reads"; exit 2; }
[ "$(readlink "/proc/$owner/exe")" = /run/slint-demo/slint-demo ] || {
    echo "unexpected UI owner; refusing to pause"; exit 2;
}
original_pan=$(cat /sys/class/graphics/fb0/pan)
kill -STOP "$owner"
sleep 1
for status in /proc/"$owner"/task/*/status; do
    if ! grep -q '^State:.*T' "$status"; then
        kill -CONT "$owner"
        echo "all UI threads must be stopped before probing"; exit 3
    fi
done
cat /proc/"$owner"/status > "$run.owner-before.txt"
dd if=/dev/fb0 bs=4096 count=1200 2>/dev/null | sha256sum > "$run.pixels-before.sha256"
load_pids=''
if [ "$stress" = 2 ]; then
    yes > /dev/null & load_pids="$!"
    yes > /dev/null & load_pids="$load_pids $!"
fi
stop_load() {
    for load_pid in $load_pids; do kill "$load_pid" 2>/dev/null || :; done
}
trap stop_load EXIT
if [ "$mode" = flips ]; then
    NX5_STOPPED_OWNER="$owner" ./probe --flips "$count" > "$run.log" 2>&1 &
elif [ "$mode" = retire ]; then
    NX5_STOPPED_OWNER="$owner" NX5_BACKUP_FILE="$run.backup.bgra" LD_LIBRARY_PATH=/run/slint-demo/lib:/lib:/usr/lib:/system/usr/lib \
        ./retire "$count" > "$run.log" 2>&1 &
elif [ "$mode" = early-reuse ]; then
    NX5_STOPPED_OWNER="$owner" NX5_BACKUP_FILE="$run.backup.bgra" LD_LIBRARY_PATH=/run/slint-demo/lib:/lib:/usr/lib:/system/usr/lib \
        ./retire "$count" --early-reuse > "$run.log" 2>&1 &
else
    NX5_STOPPED_OWNER="$owner" NX5_BACKUP_FILE="$run.backup.bgra" LD_LIBRARY_PATH=/run/slint-demo/lib:/lib:/usr/lib:/system/usr/lib \
        ./pattern "$mode" "$run.pgm" > "$run.log" 2>&1 &
fi
probe_pid=$!
echo "$probe_pid" > "$run.pid"
elapsed=0
while kill -0 "$probe_pid" 2>/dev/null; do
    if [ "$elapsed" -ge 45 ]; then
        echo "probe still running; UI remains stopped to avoid competing presenters"
        echo "inspect probe PID $probe_pid and restore original pan $original_pan before resuming UI PID $owner"
        exit 4
    fi
    sleep 1
    elapsed=$((elapsed + 1))
done
result=0
wait "$probe_pid" || result=$?
stop_load
load_pids=''
dd if=/dev/fb0 bs=4096 count=1200 2>/dev/null | sha256sum > "$run.pixels-after.sha256"
restored_pan=$(cat /sys/class/graphics/fb0/pan)
if [ "$restored_pan" != "$original_pan" ]; then
    echo "pan restoration failed; UI remains stopped, expected $original_pan, got $restored_pan"
    exit 5
fi
if [ "$mode" != flips ] && ! grep -q '_restore pixels_equal=1' "$run.log"; then
    echo "safe restoration not confirmed; UI remains stopped for recovery"; exit 7
fi
if ! cmp "$run.pixels-before.sha256" "$run.pixels-after.sha256"; then
    echo "pixel restoration failed; UI remains stopped for recovery"; exit 6
fi
kill -CONT "$owner"
sleep 1
cat /proc/"$owner"/status > "$run.owner-after.txt"
printf 'recovery original_pan=%s restored_pan=%s owner=%s probe_exit=%s\n' \
    "$original_pan" "$restored_pan" "$owner" "$result" > "$run.recovery.txt"
cat "$run.recovery.txt"
tail -n 1 "$run.log"
exit "$result"
