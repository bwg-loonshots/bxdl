#!/bin/sh
# First-time system helper placement for a verified Linux Release. Run this
# verified script as the installing user with the hash from signed release.json.
set -eu
PATH=/usr/bin:/bin
export PATH
if [ "$#" -ne 2 ]; then
    echo 'Usage: sh install-linux-helper.sh <verified-worker-file> <signed-worker-sha256>' >&2
    exit 2
fi
worker=$1
expected=$2
case "$expected" in ''|*[!0-9a-f]*) echo 'Invalid worker SHA-256' >&2; exit 2 ;; esac
[ "${#expected}" -eq 64 ] || { echo 'Invalid worker SHA-256 length' >&2; exit 2; }
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || { echo 'Linux x86_64 required' >&2; exit 3; }
[ "$(id -u)" != 0 ] || { echo 'Run as the installing user; sudo will request administrator access' >&2; exit 3; }
[ -f "$worker" ] && [ ! -L "$worker" ] || { echo 'Expected a regular worker file' >&2; exit 3; }
observed=$(sha256sum < "$worker")
[ "${observed%% *}" = "$expected" ] || { echo 'Worker digest differs from the signed release' >&2; exit 3; }

# The unprivileged shell opens the source. The privileged process receives
# only the bytes and approved digest, never a user-controlled script or path.
exec /usr/bin/sudo /bin/sh -eu -c '
    PATH=/usr/bin:/bin
    export PATH
    umask 077
    expected=$1
    dir=/usr/local/libexec/bxdl
    target=$dir/bxdl
    fail() { echo "$1" >&2; exit 3; }
    check_dir() {
        [ -d "$1" ] && [ ! -L "$1" ] && [ "$(stat -c %u -- "$1")" = 0 ] || fail "Unsafe helper parent: $1"
        mode=$(stat -c %a -- "$1")
        [ "$((0$mode & 0022))" -eq 0 ] && [ "$((0$mode & 0001))" -eq 1 ] || fail "Unsafe helper parent mode: $1"
    }
    check_existing() {
        [ -f "$target" ] && [ ! -L "$target" ] && [ "$(stat -c %u:%g:%a:%h -- "$target")" = 0:0:755:1 ] || fail "Existing helper has unexpected metadata"
        digest=$(sha256sum < "$target")
        [ "${digest%% *}" = "$expected" ] || fail "Existing helper differs; preserved for administrator review"
    }
    for p in / /usr /usr/local; do check_dir "$p"; done
    for p in /usr/local/libexec "$dir"; do
        if [ ! -e "$p" ] && [ ! -L "$p" ]; then mkdir -m 0755 -- "$p" || [ -d "$p" ]; fi
        check_dir "$p"
    done
    if [ -e "$target" ] || [ -L "$target" ]; then
        check_existing
        echo "Verified existing system helper at $target"
        exit 0
    fi
    tmp=$(mktemp -- "$dir/.bxdl-install.XXXXXXXX")
    trap '\''rm -f -- "$tmp"'\'' EXIT
    trap '\''exit 3'\'' HUP INT TERM
    head -c 536870913 > "$tmp"
    size=$(stat -c %s -- "$tmp")
    [ "$size" -gt 0 ] && [ "$size" -le 536870912 ] || fail "Worker size exceeds the package format limit"
    digest=$(sha256sum < "$tmp")
    [ "${digest%% *}" = "$expected" ] || fail "Worker changed during transfer; helper not installed"
    chown 0:0 -- "$tmp"
    chmod 0755 -- "$tmp"
    sync -- "$tmp"
    # ln refuses collisions; only complete verified bytes become executable at target.
    ln -T -- "$tmp" "$target" || fail "Helper destination appeared concurrently; rerun to verify"
    rm -- "$tmp"
    sync -f -- "$dir"
    echo "Installed verified system helper at $target"
' bxdl-install-helper "$expected" < "$worker"
