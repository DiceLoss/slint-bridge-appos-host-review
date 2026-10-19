#!/bin/sh
# Profile an existing showcase via device-local native Slint MCP, controlled over UART.
# Does not start/stop a display owner. All requests, metrics and logs stay on SD.
set -eu
DIR=${0%/*}
binary=${1:?showcase binary basename required}
tag=${2:?unique output tag required}
case "$DIR" in /tmp/sdcard/*) ;; *) echo 'must run from SD card' >&2; exit 2;; esac
case "$binary" in showcase-cpu|showcase-g2d) ;; *) exit 2;; esac
case "$tag" in ''|*[!a-zA-Z0-9_-]*) exit 2;; esac
prefix=$DIR/$tag
[ ! -e "$prefix-init.json" ] && [ ! -e "$prefix-exit-code.txt" ] || { echo 'output tag already exists' >&2; exit 2; }
phase=$prefix-phase.txt
set -- $(pidof "$binary")
[ "$#" -eq 1 ]
pid=$1
trap 'result=$?; echo done > "$phase"; echo "$result" > "$prefix-exit-code.txt"' EXIT
rpc() {
    wget -q -O "$2" --header 'Content-Type: application/json' \
      --header 'Accept: application/json, text/event-stream' --post-data "$1" \
      http://127.0.0.1:9315/mcp || return 1
    if grep -Eq '"isError"[[:space:]]*:[[:space:]]*true|"error"[[:space:]]*:' "$2"; then
        cat "$2" >&2; return 1
    fi
}
handle() {
    clean=$(sed 's/\\"/"/g' "$1")
    hgen=$(printf '%s' "$clean" | sed -n 's/.*"generation": "\([0-9][0-9]*\)".*/\1/p')
    hindex=$(printf '%s' "$clean" | sed -n 's/.*"index": "\([0-9][0-9]*\)".*/\1/p')
    [ -n "$hgen" ] && [ -n "$hindex" ]
}
find() {
    request=$(printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_elements_by_id","arguments":{"windowHandle":{"generation":"%s","index":"%s"},"elementsId":"%s"}}}' "$wgen" "$windex" "$1")
    rpc "$request" "$prefix-response.json"
    handle "$prefix-response.json"
}
nav() {
    find "Showcase::nav-$1"
    request=$(printf '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"click_element","arguments":{"elementHandle":{"generation":"%s","index":"%s"}}}}' "$hgen" "$hindex")
    rpc "$request" "$prefix-response.json"
}
i=0
until rpc '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"f1-uart-profile","version":"1"}}}' "$prefix-init.json"; do
    i=$((i+1)); [ "$i" -lt 20 ]; sleep 1
done
rpc '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' "$prefix-tools.json"
rpc '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_windows","arguments":{}}}' "$prefix-window-list.json"
handle "$prefix-window-list.json"
wgen=$hgen; windex=$hindex
request=$(printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_window_properties","arguments":{"windowHandle":{"generation":"%s","index":"%s"}}}}' "$wgen" "$windex")
rpc "$request" "$prefix-window.json"
available=$(awk '/^MemAvailable:/{print $2}' /proc/meminfo)
if [ "$available" -ge 16384 ]; then
    request=$(printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"take_screenshot","arguments":{"windowHandle":{"generation":"%s","index":"%s"},"imageMimeType":"image/png"}}}' "$wgen" "$windex")
    rpc "$request" "$prefix-screenshot.json"
    echo "screenshot_saved available_kib=$available"
else
    echo "screenshot_skipped available_kib=$available"
fi
echo warmup > "$phase"
observer=$("$DIR/run-detached" "$prefix-resources.csv" "$DIR/observe-resources" "$pid" 160 "$phase")
echo "app_pid=$pid observer_pid=$observer"
for entry in controls_idle:controls overview:overview motion:motion; do
    name=${entry%:*}; page=${entry#*:}
    nav "$page"; sleep 3
    echo "$name" > "$phase"; sleep 20; echo warmup > "$phase"
    echo "phase_done=$name"
done
available=$(awk '/^MemAvailable:/{print $2}' /proc/meminfo)
[ "$available" -ge 8192 ]
nav rich-list; sleep 3
find 'RichList::listing'
for direction in negative positive; do
    delta=-30; [ "$direction" = negative ] || delta=30
    printf '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"scroll_element","arguments":{"elementHandle":{"generation":"%s","index":"%s"},"deltaY":%s}}}' "$hgen" "$hindex" "$delta" > "$prefix-$direction.json"
done
echo rich_scroll > "$phase"
"$DIR/mcp-scroll" "$prefix-negative.json" "$prefix-positive.json" 20 > "$prefix-scroll.log"
echo warmup > "$phase"
echo phase_done=rich_scroll
for page in devices media lab scaling clock overview; do nav "$page"; sleep 1; done
echo done > "$phase"
echo PROFILE_DONE
