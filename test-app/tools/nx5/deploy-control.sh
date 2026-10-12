#!/bin/sh
# Shared lock and verified archival removal of a transaction directory.
set -eu
fail() { echo "nx5-deploy: $*" >&2; exit 1; }

control_manifest() {
    for file in "$work"/* "$work"/.[!.]* "$work"/..?*; do
        [ -e "$file" ] || [ -L "$file" ] || continue
        [ "$file" != "$work/purge.sha256" ] || continue
        [ -f "$file" ] && [ ! -L "$file" ] || fail "unexpected control entry: $file"
        sha256sum "$file" || fail "cannot hash control file: $file"
    done
}

[ "$#" = 2 ] || { [ "$#" = 3 ] && [ "$1" = seal ]; } || fail "usage: deploy-device.sh COMMAND WORK_DIRECTORY"
mode=$1
work=$2
printf '%s\n' "$work" | grep -Eq '^/tmp/nx5-deploy-[0-9a-f]{32}$' || fail "invalid work directory"
[ -d "$work" ] && [ ! -L "$work" ] || fail "missing work directory"
[ "$(readlink -f "$work")" = "$work" ] || fail "noncanonical work directory"
# Never unlink this inode: the kernel releases the lock even after SIGKILL/OOM.
lock=/tmp/nx5-deploy-cleanup.lock
[ ! -d "$lock" ] || fail "legacy lock directory exists; verify old commands have exited before removing it"
[ ! -L "$lock" ] || fail "invalid lock file"
if [ ! -e "$lock" ]; then (umask 022; set -C; : > "$lock") 2>/dev/null || true; fi
[ -f "$lock" ] || fail "missing lock file"
exec 9< "$lock"
flock -n 9 || fail "another inventory/cleanup is running"
case "$mode" in
    control-manifest) control_manifest; exit ;;
    purge)
        current_manifest=$(control_manifest) || fail "cannot verify control files"
        [ "$current_manifest" = "$(cat "$work/purge.sha256")" ] || fail "control files changed since archive"
        rm -rf -- "$work"
        exit
        ;;
esac
