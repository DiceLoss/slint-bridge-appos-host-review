#!/bin/sh
# Device-side deployment controls. Never loads the SDK or stops the display.
set -eu

fail() { echo "nx5-deploy: $*" >&2; exit 1; }

managed_file() {
    case "$1" in
        /run/slint-bridge-*/*|/tmp/slint-bridge-*/*) ;;
        *) return 1 ;;
    esac
    [ "$(readlink -f "$1")" = "$1" ] && [ -f "$1" ] && [ ! -L "$1" ]
}

resolve_paths() (
    # Follow dependency links inside retained directories, including links to
    # other directories. The visited set also terminates directory-link cycles.
    rm -f "$work/resolve.todo" "$work/resolve.seen"
    cat "$1" > "$work/resolve.todo"
    : > "$work/resolve.seen"
    while IFS= read -r path; do
        case "$path" in /*) ;; *) continue ;; esac
        grep -Fxq -- "$path" "$work/resolve.seen" && continue
        printf '%s\n' "$path" >> "$work/resolve.seen"
        resolved=$(readlink -f "$path") || continue
        if [ "$resolved" != "$path" ]; then
            printf '%s\n' "$resolved" >> "$work/resolve.todo"
        elif [ -d "$path" ]; then
            find "$path" -type l -exec readlink -f {} \; >> "$work/resolve.todo"
        fi
    done < "$work/resolve.todo"
    sort -u "$work/resolve.seen"
)

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
                        /*) printf '%s\n' "$dependency" ;;
                        *) base=$(readlink "$proc/cwd") || continue
                           printf '%s\n' "$base/$dependency" ;;
                    esac
                done
        fi
    done | sed 's/\\040/ /g' | sort -u > "$work/live.paths"
    [ ! -f "$work/retain.paths" ] || cat "$work/retain.paths" >> "$work/live.paths"
    resolve_paths "$work/live.paths"
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

same_boot() {
    [ "$(cat "$work/boot-id")" = "$(cat /proc/sys/kernel/random/boot_id)" ] ||
        fail "device rebooted; discard this plan"
}

verify_plan() {
    expected=$(sha256sum "$work/boot-id" "$work/retain.paths" "$work/retained.sha256" "$work/delete.sha256")
    [ "$(cat "$1")" = "$expected" ] || fail "cleanup plan is incomplete or changed"
}

. "${2:?missing work directory}/control.sh"
case "$mode" in inventory|cleanup)
    [ "$(id -u)" = 0 ] || fail "run inventory/cleanup in the existing root terminal" ;;
esac

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
        # An interrupted inventory must never be mistaken for a complete one.
        : > "$work/inventory.ready"
        echo "nx5-deploy: inventory ready at $work"
        ;;
    begin)
        same_boot
        [ -f "$work/inventory.ready" ] || fail "inventory incomplete"
        for name in preparing plan.ready retain.paths retained.sha256 delete.sha256 cleaned; do
            [ ! -e "$work/$name" ] || fail "transaction already prepared or interrupted; discard and inspect again"
        done
        : > "$work/preparing"
        ;;
    protect)
        same_boot
        resolve_paths "$work/retain.paths"
        ;;
    seal)
        same_boot
        [ ! -e "$work/plan.ready" ] && [ ! -e "$work/cleaned" ] || fail "plan already sealed"
        [ "$(sha256sum "$work/plan.ready.pending" | cut -d ' ' -f 1)" = "${3:?missing plan digest}" ] || fail "plan transfer checksum mismatch"
        verify_plan "$work/plan.ready.pending"
        mv "$work/plan.ready.pending" "$work/plan.ready"
        ;;
    cleanup)
        same_boot
        [ ! -e "$work/cleaned" ] || fail "plan already applied"
        [ -f "$work/plan.ready" ] || fail "cleanup plan was not completely archived/published"
        verify_plan "$work/plan.ready"
        protected_paths > "$work/protected-now.paths"
        sha256sum -c "$work/retained.sha256" >/dev/null || fail "current/recovery program changed"
        verify_candidates
        # Hashing a large archive takes time; refresh live and policy links.
        protected_paths > "$work/protected-now.paths"
        while IFS= read -r line; do
            hash=${line%% *}
            path=${line#"$hash  "}
            is_protected "$path" && fail "file became active or retained: $path"
        done < "$work/delete.sha256"
        while IFS= read -r line; do
            hash=${line%% *}
            path=${line#"$hash  "}
            managed_file "$path" || fail "file moved before deletion: $path"
            [ "$(sha256sum "$path")" = "$line" ] || fail "file changed before deletion: $path"
            rm -- "$path"
        done < "$work/delete.sha256"
        cat /proc/meminfo > "$work/after.meminfo"
        sha256sum "$work/plan.ready" | cut -d ' ' -f 1 > "$work/cleaned"
        echo "nx5-deploy: archived files removed; current/recovery and active files retained"
        ;;
    *) fail "unknown command: $mode" ;;
esac
