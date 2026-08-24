#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t76-standard-cargo"
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
probe_id="${BASHPID:-$$}-${RANDOM}"
probe_root="$project_root/target/t76-command-matrix/$probe_id"
cargo_home="$probe_root/cargo-home"
workspace="$probe_root/workspace"
limited_endpoint="$probe_root/service/agent.sock"
cargo_bin=$(command -v cargo || true)

fail() {
    printf '%s\n' "AD-T76: $1" >&2
    exit 1
}

if [[ $(id -u) -eq 0 ]]; then
    printf '%s\n' 'AD-T76: the release installer intentionally refuses root.' >&2
    exit 77
fi
if [[ -z "$cargo_bin" ]]; then
    printf '%s\n' 'AD-T76: cargo is required for the standard-command matrix.' >&2
    exit 77
fi
if ! "$cargo_bin" clippy --version >/dev/null 2>&1; then
    printf '%s\n' 'AD-T76: the clippy Cargo subcommand is required.' >&2
    exit 77
fi
if [[ ! -x "$release_binary" ]]; then
    fail 'the native release binary must be built before the matrix.'
fi
if [[ ! -r "$fixture_source/Cargo.lock" ]]; then
    fail 'the locked standard-command fixture is unavailable.'
fi
if rg -q 'rustc(-workspace)?-wrapper' "$fixture_source/.cargo/config.toml"; then
    fail 'the fixture must not select a local wrapper.'
fi

umask 077
mkdir -p "$cargo_home" "$workspace"
cp -R "$fixture_source/." "$workspace/"

ANTDELIRIUM_LIMITED_SOCKET="$limited_endpoint" \
    CARGO_HOME="$cargo_home" \
    "$release_binary" install \
    >"$probe_root/install.stdout" \
    2>"$probe_root/install.stderr" || fail 'release installation failed.'

installed_binary="$cargo_home/bin/antdelirium"
config="$cargo_home/config.toml"
[[ -x "$installed_binary" ]] || fail 'the installed wrapper is unavailable.'
[[ -f "$config" ]] || fail 'the Cargo configuration was not created.'
[[ ! -e "$cargo_home/bin/cargo" ]] || fail 'the rustup cargo proxy was replaced.'
rg -F -q -- "rustc-workspace-wrapper = \"$installed_binary\"" "$config" \
    || fail 'the installed configuration does not select the wrapper.'
rg -F -q -- "ANTDELIRIUM_LIMITED_SOCKET = { value = \"$limited_endpoint\", force = true }" "$config" \
    || fail 'the installed configuration does not inject the limited endpoint.'

run_standard_command() {
    local command_name=$1
    local command_stdout="$probe_root/cargo-$command_name.stdout"
    local command_stderr="$probe_root/cargo-$command_name.stderr"
    local target_dir="$probe_root/target-$command_name"

    if ! (
        cd "$workspace"
        env \
            -u RUSTC \
            -u RUSTC_WRAPPER \
            -u RUSTC_WORKSPACE_WRAPPER \
            -u CARGO_BUILD_RUSTC \
            -u CARGO_BUILD_RUSTC_WRAPPER \
            -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
            -u ANTDELIRIUM_WRAPPER_DEPTH \
            -u ANTDELIRIUM_LIMITED_SOCKET \
            -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
            CARGO_HOME="$cargo_home" \
            CARGO_TARGET_DIR="$target_dir" \
            "$cargo_bin" "$command_name"
    ) >"$command_stdout" 2>"$command_stderr"; then
        sed -n '1,80p' "$command_stderr" >&2
        fail "cargo $command_name failed."
    fi
}

run_standard_command check
run_standard_command build
run_standard_command test
run_standard_command run
run_standard_command clippy

set +e
(
    cd "$workspace"
    env \
        -u RUSTC \
        -u RUSTC_WRAPPER \
        -u RUSTC_WORKSPACE_WRAPPER \
        -u CARGO_BUILD_RUSTC \
        -u CARGO_BUILD_RUSTC_WRAPPER \
        -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
        -u ANTDELIRIUM_LIMITED_SOCKET \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        ANTDELIRIUM_WRAPPER_DEPTH=invalid \
        CARGO_HOME="$cargo_home" \
        CARGO_TARGET_DIR="$probe_root/target-wrapper-proof" \
        "$cargo_bin" check
) >"$probe_root/wrapper-proof.stdout" 2>"$probe_root/wrapper-proof.stderr"
proof_status=$?
set -e
[[ $proof_status -ne 0 ]] || fail 'Cargo ignored the installed wrapper configuration.'
rg -F -q -- 'AD-CONFIG-0001' "$probe_root/wrapper-proof.stderr" \
    || fail 'the installed wrapper did not produce its closed configuration error.'

[[ $(<"$probe_root/cargo-run.stdout") == "t76-standard-commands" ]] \
    || fail 'cargo run changed the fixture output.'
rg -F -q -- 'test result: ok.' "$probe_root/cargo-test.stdout" \
    || fail 'cargo test did not execute the fixture test.'

for forbidden in \
    "$workspace/guard" \
    "$workspace/login.json" \
    "$workspace/AC_Ant-Delirium_props" \
    "$workspace/plugins"
do
    [[ ! -e "$forbidden" ]] || fail 'a standard Cargo command created obsolete project state.'
done

printf '%s\n' 'AD-T76: cargo check, build, test, run and clippy passed through the installed integration.'
