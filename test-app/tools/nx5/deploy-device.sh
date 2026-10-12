#!/bin/sh
# Root-side inventory/cleanup. Never loads the SDK or stops the display.
set -eu

fail() { echo "nx5-deploy: $*" >&2; exit 1; }

managed_file() {
    case "$1" in
        /run/slint-bridge-*/*|/tmp/slint-bridge-*/*) ;;
        *) return 1 ;;
    esac
    [ "$(readlink -f "$1")" = "$1" ] && [ -f "$1" ] && [ ! -L "$1" ]
}

protected_paths() {
    # Include lazy-loaded fonts/libraries, not just mappings already in use.
    for proc in /proc/[0-9]*; do
        for entry in "$proc/exe" "$proc"/fd/*; do
            readlink "$entry" 2>/dev/null || true
        done
        if [ -r "$proc/maps" ]; then
            sed -n 's|^[^/]*\(/.*\)|\1|p' "$proc/maps" 2>/dev/null || true
        fi
        if [ -r "$proc/environ" ]; then
            tr '\000' '\n' < "$proc/environ" 2>/dev/null |
                sed -n 's/^SLINT_DEFAULT_FONT=//p; s/^LD_LIBRARY_PATH=//p' |
                tr ':' '\n' |
                while IFS= read -r dependency; do
                    case "$dependency" in
                        /*) printf '%s\n' "$dependency"; readlink -f "$dependency" || true ;;
                        *) base=$(readlink "$proc/cwd") || continue
                           readlink -f "$base/$dependency" || true ;;
                    esac
                done
        fi
    done | sed 's/\\040/ /g' | sort -u
}

is_protected() {
    while IFS= read -r retained; do
        [ -n "$retained" ] || continue
        case "$1" in "$retained"|"${retained%/}"/*) return 0 ;; esac
    done < "$work/protected-now.paths"
    case "$1" in *.lock) return 0 ;; esac
    return 1
}

verify_candidates() {
    while IFS= read -r line; do
        hash=${line%% *}
        path=${line#"$hash  "}
        managed_file "$path" || fail "not a canonical regular managed file: $path"
        is_protected "$path" && fail "file became active or retained: $path"
        actual=$(sha256sum "$path")
        [ "$actual" = "$line" ] || fail "file changed since archive: $path"
    done < "$work/delete.sha256"
}

[ "$(id -u)" = 0 ] || fail "run inventory/cleanup in the existing root terminal"
[ "$#" = 2 ] || fail "usage: deploy-device.sh inventory|cleanup WORK_DIRECTORY"
mode=$1
work=$2
case "$work" in /tmp/nx5-deploy-*) ;; *) fail "invalid work directory" ;; esac
[ -d "$work" ] && [ ! -L "$work" ] || fail "missing work directory"
[ "$(readlink -f "$work")" = "$work" ] || fail "noncanonical work directory"
mkdir /tmp/nx5-deploy-cleanup.lock || fail "another inventory/cleanup is running"
trap 'rmdir /tmp/nx5-deploy-cleanup.lock' EXIT

case "$mode" in
    inventory)
        [ ! -e "$work/boot-id" ] || fail "inventory already exists; start a new transaction"
        cat /proc/sys/kernel/random/boot_id > "$work/boot-id"
        cat /proc/meminfo > "$work/before.meminfo"
        protected_paths > "$work/protected.paths"
        : > "$work/files.sha256"
        for root in /run/slint-bridge-* /tmp/slint-bridge-*; do
            [ -d "$root" ] && [ ! -L "$root" ] || continue
            find "$root" -type f -exec sha256sum {} \; >> "$work/files.sha256"
        done
        echo "nx5-deploy: inventory ready at $work"
        ;;
    cleanup)
        [ "$(cat "$work/boot-id")" = "$(cat /proc/sys/kernel/random/boot_id)" ] ||
            fail "device rebooted; discard this plan"
        [ ! -e "$work/cleaned" ] || fail "plan already applied"
        protected_paths > "$work/protected-now.paths"
        cat "$work/retain.paths" >> "$work/protected-now.paths"
        sha256sum -c "$work/retained.sha256" >/dev/null || fail "current/recovery program changed"
        verify_candidates
        # Hashing a large archive takes time; refresh live references before deletion.
        protected_paths > "$work/protected-now.paths"
        cat "$work/retain.paths" >> "$work/protected-now.paths"
        while IFS= read -r line; do
            hash=${line%% *}
            path=${line#"$hash  "}
            is_protected "$path" && fail "file became active: $path"
        done < "$work/delete.sha256"
        while IFS= read -r line; do
            hash=${line%% *}
            path=${line#"$hash  "}
            managed_file "$path" || fail "file moved before deletion: $path"
            [ "$(sha256sum "$path")" = "$line" ] || fail "file changed before deletion: $path"
            rm -- "$path"
        done < "$work/delete.sha256"
        cat /proc/meminfo > "$work/after.meminfo"
        cp "$work/boot-id" "$work/cleaned"
        echo "nx5-deploy: archived files removed; current/recovery and active files retained"
        ;;
    *) fail "unknown command: $mode" ;;
esac
