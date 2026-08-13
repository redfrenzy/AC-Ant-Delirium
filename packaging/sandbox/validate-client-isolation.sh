#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_container="$project_root/src/tests/fixtures/t48-protected-container"
fixture_root="$fixture_container/canonical"
isolated_fixture_container="/workspace/src/tests/fixtures/t48-protected-container"
isolated_fixture_root="$isolated_fixture_container/canonical"
host_pid=$$

if ! command -v bwrap >/dev/null 2>&1; then
    printf '%s\n' 'AD-T48: bubblewrap is required for the client isolation probe.' >&2
    exit 77
fi
if [[ ! -r "$fixture_root/secret-state.db" ]]; then
    printf '%s\n' 'AD-T48: protected fixture is unavailable before isolation.' >&2
    exit 78
fi

bwrap \
    --die-with-parent \
    --new-session \
    --unshare-user \
    --unshare-pid \
    --unshare-net \
    --unshare-ipc \
    --unshare-uts \
    --unshare-cgroup-try \
    --disable-userns \
    --uid 65534 \
    --gid 65534 \
    --cap-drop ALL \
    --clearenv \
    --setenv PATH /usr/bin:/bin \
    --setenv ANTDELIRIUM_PROBE_ROOT "$isolated_fixture_root" \
    --setenv ANTDELIRIUM_SERVICE_ROOT /var/lib/antdelirium \
    --setenv ANTDELIRIUM_HOST_PID "$host_pid" \
    --dir /usr \
    --ro-bind /usr /usr \
    --symlink usr/bin /bin \
    --symlink usr/lib /lib \
    --symlink usr/lib64 /lib64 \
    --dir /etc \
    --ro-bind /etc /etc \
    --dir /workspace \
    --ro-bind "$project_root" /workspace \
    --tmpfs "$isolated_fixture_container" \
    --tmpfs /var \
    --tmpfs /run \
    --tmpfs /tmp \
    --dev /dev \
    --proc /proc \
    --chdir /workspace \
    /usr/bin/bash -ceu '
        reject_access() {
            if "$@" >/dev/null 2>&1; then
                printf "%s\n" "AD-T48: an isolated client reached a protected resource." >&2
                exit 1
            fi
        }

        test "$(/usr/bin/id -u)" = 65534
        test "$(/usr/bin/id -g)" = 65534
        test "$(/usr/bin/awk "/^CapEff:/ { print \$2 }" /proc/self/status)" = 0000000000000000
        reject_access /usr/bin/unshare --user /usr/bin/true
        test -r Cargo.toml
        test ! -e "/proc/$ANTDELIRIUM_HOST_PID"

        for protected_root in "$ANTDELIRIUM_PROBE_ROOT" "$ANTDELIRIUM_SERVICE_ROOT"; do
            reject_access /usr/bin/stat -- "$protected_root"
            reject_access /usr/bin/test -e "$protected_root"
            reject_access /usr/bin/test -r "$protected_root"
            reject_access /usr/bin/test -x "$protected_root"
            reject_access /usr/bin/readlink -- "$protected_root"
            reject_access /usr/bin/realpath -e -- "$protected_root"
            reject_access /usr/bin/ls -la -- "$protected_root"
            reject_access /usr/bin/dd if="$protected_root/secret-state.db" of=/dev/null status=none
        done

        printf "%s\n" "AD-T48: isolated client cannot resolve or access protected state."
    '
