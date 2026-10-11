#!/bin/sh
# Read-only sampling of the selected benchmark process. Ignore the first
# five-second interval when comparing steady-state CPU usage.
set -eu
pid=$1
case "$pid" in ''|*[!0-9]*) exit 2;; esac
for sample in 0 1 2 3 4; do
    cat /proc/uptime
    cat "/proc/$pid/stat"
    if [ "$sample" -lt 4 ]; then sleep 5; fi
done
