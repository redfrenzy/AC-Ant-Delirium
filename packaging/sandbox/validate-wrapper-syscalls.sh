#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t66-cargo-matrix"
protected_root="$project_root/src/tests/fixtures/t48-protected-container/canonical"
probe_id="${BASHPID:-$$}-${RANDOM}"
probe_root="$project_root/target/t68-syscall-probe/$probe_id"
workspace="$probe_root/workspace"
target_dir="$probe_root/cargo-target"
trace_file="$probe_root/wrapper-files.trace"
trace_stderr="$probe_root/strace.stderr"
cargo_stdout="$probe_root/cargo.stdout"
cargo_stderr="$probe_root/cargo.stderr"
limited_endpoint=/run/antdelirium/agent.sock
wrapper_default="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
wrapper=${ANTDELIRIUM_PROBE_BINARY:-$wrapper_default}
cargo_home_dir=${CARGO_HOME:-$HOME/.cargo}

fail() {
    printf '%s\n' "AD-T68: $1" >&2
    exit 1
}

if ! command -v strace >/dev/null 2>&1; then
    printf '%s\n' 'AD-T68: strace is required for the wrapper syscall probe.' >&2
    exit 77
fi
if ! command -v cargo >/dev/null 2>&1; then
    printf '%s\n' 'AD-T68: cargo is required for the wrapper syscall probe.' >&2
    exit 77
fi
if [[ ! -x "$wrapper" ]]; then
    printf '%s\n' 'AD-T68: the native release wrapper must be built before tracing.' >&2
    exit 78
fi
if [[ ! -r "$protected_root/secret-state.db" ]]; then
    printf '%s\n' 'AD-T68: the nonempty protected fixture is unavailable.' >&2
    exit 78
fi
if [[ ! -r "$fixture_source/Cargo.lock" ]]; then
    printf '%s\n' 'AD-T68: the locked Cargo matrix fixture is unavailable.' >&2
    exit 78
fi

umask 077
mkdir -p "$workspace" "$target_dir"
cp -R "$fixture_source/." "$workspace/"
wrapper=$(realpath -- "$wrapper")
cargo_bin=$(command -v cargo)
relative_protected_root=$(realpath --relative-to="$workspace" "$protected_root")

set +e
strace \
    -f \
    -qq \
    -s 4096 \
    -e trace=%file,connect \
    -o "$trace_file" \
    env \
        -u RUSTC_WRAPPER \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        RUSTC_WORKSPACE_WRAPPER="$wrapper" \
        ANTDELIRIUM_LIMITED_SOCKET="$limited_endpoint" \
        CARGO_TARGET_DIR="$target_dir" \
        "$cargo_bin" check \
            --manifest-path "$workspace/Cargo.toml" \
            --quiet \
            --locked \
            --offline \
            --workspace \
            --all-targets \
            --target x86_64-unknown-linux-gnu \
            -j 4 \
        >"$cargo_stdout" \
        2>"$cargo_stderr" \
    2>"$trace_stderr"
probe_status=$?
set -e

if [[ $probe_status -ne 0 || ! -s "$trace_file" ]]; then
    printf '%s\n' 'AD-T68: syscall tracing was denied or governed cargo check failed.' >&2
    exit 77
fi

rg -F -q -- "$wrapper" "$trace_file" || fail 'the trace did not prove wrapper execution.'
rg -F -q -- "$limited_endpoint" "$trace_file" || fail 'the trace did not follow the audit-only child.'

reject_trace_literal() {
    local literal=$1
    local label=$2
    if rg -F -q -- "$literal" "$trace_file"; then
        fail "a wrapper descendant referenced $label."
    fi
}

reject_trace_literal "$protected_root" 'the protected fixture root'
reject_trace_literal "$relative_protected_root" 'a relative traversal to protected state'
reject_trace_literal 't48-protected-container/canonical' 'the protected fixture layout'
reject_trace_literal 'secret-state.db' 'the protected fixture payload'
reject_trace_literal '/var/lib/antdelirium' 'the system service state root'
reject_trace_literal "$cargo_home_dir/antdelirium" 'the user-local state root'

printf '%s\n' 'AD-T68: wrapper descendants did not access or test protected state.'
