#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t76-standard-cargo"
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
evidence_schema="$project_root/docs/contracts/v1/fail-open-rollout-evidence-v1.schema.json"
malformed_peer="$script_dir/malformed-limited-peer.py"
limited_socket=/run/antdelirium/agent.sock
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t91-fail-open-rollout/$run_id"
socket_root="$project_root/target/t91-rollout-sockets"
malformed_socket="$socket_root/${BASHPID:-$$}.sock"
cargo_bin=$(command -v cargo || true)
rustc_bin=
server_pid=

fail() {
    printf '%s\n' "AD-T91: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill -TERM "$server_pid" 2>/dev/null
        wait "$server_pid" 2>/dev/null
    fi
    {
        printf 'contract=antdelirium-fail-open-rollout-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T91: logs=$log_root"
    exit "$exit_code"
}

for command in cp cut date find id jq kill ln mkdir mv python3 rg rustc sha256sum sleep stat; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ $(id -u) -ne 0 ]] || fail 'run this rollout gate as the non-root operator.'
[[ -n "$cargo_bin" ]] || fail 'Cargo is required for the fail-open rollout.'
rustc_bin=$(rustc --print sysroot 2>/dev/null)/bin/rustc
[[ -x "$rustc_bin" && ! -L "$rustc_bin" ]] \
    || fail 'the resolved native rustc is unavailable.'
[[ -x "$release_binary" && ! -L "$release_binary" ]] \
    || fail 'build the reviewed native release before the rollout.'
[[ -f "$fixture_source/Cargo.lock" && ! -L "$fixture_source" ]] \
    || fail 'the maintained synthetic fixture is unavailable.'
[[ -f "$evidence_schema" && -f "$malformed_peer" ]] \
    || fail 'the rollout evidence contract or malformed peer helper is unavailable.'
[[ -S "$limited_socket" ]] || fail 'the installed limited socket is unavailable.'
python3 -B "$malformed_peer" --check \
    || fail 'the malformed peer helper failed its static check.'
jq empty "$evidence_schema" || fail 'the rollout evidence schema is invalid.'

umask 077
mkdir -p "$log_root" "$socket_root"
trap finish EXIT

prepare_case() {
    local name=$1
    case_root="$log_root/$name"
    case_workspace="$case_root/workspace"
    case_target="$case_root/target"
    case_cargo_home="$case_root/cargo-home"
    mkdir -p "$case_workspace" "$case_target" "$case_cargo_home"
    cp -R "$fixture_source/." "$case_workspace/"
}

write_endpoint_config() {
    local endpoint=$1
    case_endpoint_config="$case_root/endpoint-config.toml"
    printf '[env]\nANTDELIRIUM_LIMITED_SOCKET = { value = "%s", force = true }\n' \
        "$endpoint" >"$case_endpoint_config"
}

assert_wrapper_participated() {
    local stderr_file=$1
    rg -F -q -- "$release_binary" "$stderr_file" \
        || fail 'a rollout case did not invoke the reviewed wrapper.'
    rg -F -q -- 'CARGO_PRIMARY_PACKAGE=1' "$stderr_file" \
        || fail 'a rollout case did not compile the primary fixture package.'
}

assert_fail_open_success() {
    local label=$1
    local stderr_file=$2
    local workspace=$3
    [[ ! -e "$workspace/guard" ]] \
        || fail "$label unexpectedly materialized governance output."
    if rg -q 'antdelirium\[AD-' "$stderr_file"; then
        fail "$label exposed a governance error on the compiler stream."
    fi
}

run_isolated_check() {
    local endpoint_value=$1
    local -a unset_environment=(
        -u RUSTC
        -u RUSTC_WRAPPER
        -u CARGO_BUILD_RUSTC
        -u CARGO_BUILD_RUSTC_WRAPPER
        -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER
        -u ANTDELIRIUM_WRAPPER_DEPTH
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD
    )
    write_endpoint_config "$endpoint_value"
    (
        cd "$case_workspace"
        env \
            "${unset_environment[@]}" \
            CARGO_HOME="$case_cargo_home" \
            CARGO_TARGET_DIR="$case_target" \
            RUSTC_WORKSPACE_WRAPPER="$release_binary" \
            "$cargo_bin" --config "$case_endpoint_config" \
                -vv check --locked --offline
    ) >"$case_root/cargo.stdout" 2>"$case_root/cargo.stderr"
}

printf '%s\n' 'AD-T91: proving endpoint absence is non-blocking in audit-only rollout.'
prepare_case endpoint-absent
(
    cd "$case_workspace"
    env \
        -u ANTDELIRIUM_LIMITED_SOCKET \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        CARGO_PRIMARY_PACKAGE=1 \
        CARGO_CRATE_NAME=t76_standard_commands \
        CARGO_MANIFEST_DIR="$case_workspace" \
        CARGO_MANIFEST_PATH="$case_workspace/Cargo.toml" \
        "$release_binary" "$rustc_bin" \
            --crate-name t76_standard_commands \
            --edition=2021 \
            src/main.rs \
            --crate-type bin \
            --emit=metadata \
            --out-dir "$case_target"
) >"$case_root/cargo.stdout" 2>"$case_root/cargo.stderr"
assert_fail_open_success endpoint-absent "$case_root/cargo.stderr" "$case_workspace"

printf '%s\n' 'AD-T91: proving unavailable service transport is non-blocking.'
prepare_case service-unavailable
run_isolated_check "$case_root/unavailable.sock"
assert_wrapper_participated "$case_root/cargo.stderr"
assert_fail_open_success service-unavailable "$case_root/cargo.stderr" "$case_workspace"

printf '%s\n' 'AD-T91: proving malformed limited protocol is non-blocking.'
prepare_case malformed-protocol
peer_report="$case_root/malformed-peer.env"
python3 -B "$malformed_peer" "$malformed_socket" "$peer_report" \
    >"$case_root/peer.stdout" 2>"$case_root/peer.stderr" &
server_pid=$!
peer_ready=false
for ((attempt = 1; attempt <= 100; attempt++)); do
    if [[ -S "$malformed_socket" && -f "$peer_report.ready" ]]; then
        peer_ready=true
        break
    fi
    sleep 0.02
done
[[ "$peer_ready" == true ]] || fail 'the malformed peer did not become ready.'
run_isolated_check "$malformed_socket"
wait "$server_pid" || fail 'the malformed peer did not complete its protocol fault.'
server_pid=
rg -F -q 'accepted=1' "$peer_report" \
    || fail 'the malformed peer did not observe the audit-only client.'
assert_wrapper_participated "$case_root/cargo.stderr"
assert_fail_open_success malformed-protocol "$case_root/cargo.stderr" "$case_workspace"

printf '%s\n' 'AD-T91: measuring false positives over three valid installed observations.'
false_positive_count=0
valid_observation_count=0
for cargo_command in check build test; do
    prepare_case "valid-$cargo_command"
    write_endpoint_config "$limited_socket"
    if ! (
        cd "$case_workspace"
        env \
            -u RUSTC \
            -u RUSTC_WRAPPER \
            -u CARGO_BUILD_RUSTC \
            -u CARGO_BUILD_RUSTC_WRAPPER \
            -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
            -u ANTDELIRIUM_WRAPPER_DEPTH \
            -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
            CARGO_HOME="$case_cargo_home" \
            CARGO_TARGET_DIR="$case_target" \
            RUSTC_WORKSPACE_WRAPPER="$release_binary" \
            "$cargo_bin" --config "$case_endpoint_config" \
                -vv "$cargo_command" --locked --offline
    ) >"$case_root/cargo-$cargo_command.stdout" \
        2>"$case_root/cargo-$cargo_command.stderr"; then
        fail "valid cargo $cargo_command was blocked during fail-open rollout."
    fi
    assert_wrapper_participated "$case_root/cargo-$cargo_command.stderr"
    projections_ready=false
    for ((attempt = 1; attempt <= 100; attempt++)); do
        if [[ -f "$case_workspace/guard/to-do_list.json" \
            && -f "$case_workspace/guard/governance_error.json" ]]; then
            projections_ready=true
            break
        fi
        sleep 0.05
    done
    [[ "$projections_ready" == true ]] \
        || fail 'the valid installed observation did not publish both projections.'
    jq -e '
        .contract_version == "agent-projection-v1" and
        .artifact_kind == "governance_errors" and
        (.violations | type == "array")
    ' "$case_workspace/guard/governance_error.json" >/dev/null \
        || fail 'the valid observation produced an invalid governance projection.'
    observed=$(jq '.violations | length' "$case_workspace/guard/governance_error.json")
    false_positive_count=$((false_positive_count + observed))
    valid_observation_count=$((valid_observation_count + 1))
done
[[ $valid_observation_count -eq 3 && $false_positive_count -eq 0 ]] \
    || fail 'the valid synthetic rollout produced a governance false positive.'

printf '%s\n' 'AD-T91: proving projection materialization failure is non-blocking.'
# Reuse the independently registered valid-test fixture from the last rollout
# observation, but replace only its public projection destination with a
# project-local symlink. The canonical service state remains untouched.
mv "$case_workspace/guard" "$case_workspace/guard-valid"
mkdir -p "$case_workspace/blocked-guard-target"
ln -s blocked-guard-target "$case_workspace/guard"
write_endpoint_config "$limited_socket"
if ! (
    cd "$case_workspace"
    env \
        -u RUSTC \
        -u RUSTC_WRAPPER \
        -u CARGO_BUILD_RUSTC \
        -u CARGO_BUILD_RUSTC_WRAPPER \
        -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        CARGO_HOME="$case_cargo_home" \
        CARGO_TARGET_DIR="$case_root/target-projection-conflict" \
        RUSTC_WORKSPACE_WRAPPER="$release_binary" \
        "$cargo_bin" --config "$case_endpoint_config" \
            -vv check --locked --offline
) >"$case_root/projection-conflict.stdout" \
    2>"$case_root/projection-conflict.stderr"; then
    fail 'a projection materialization failure blocked the compiler.'
fi
assert_wrapper_participated "$case_root/projection-conflict.stderr"
sleep 1
[[ -L "$case_workspace/guard" ]] \
    || fail 'the audit-only worker replaced an untrusted projection symlink.'
[[ -z $(find "$case_workspace/blocked-guard-target" -mindepth 1 -maxdepth 1 -print -quit) ]] \
    || fail 'the audit-only worker wrote through an untrusted projection symlink.'

printf '%s\n' 'AD-T91: proving compiler failure keeps precedence over audit degradation.'
prepare_case compiler-failure
printf '%s\n' 'fn main() { let _: u8 = "not-a-number"; }' >"$case_workspace/src/main.rs"
mkdir -p "$case_root/target-direct" "$case_root/target-wrapped"
set +e
(
    cd "$case_workspace"
    env \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        -u ANTDELIRIUM_LIMITED_SOCKET \
        "$rustc_bin" \
            --crate-name t76_standard_commands \
            --edition=2021 \
            src/main.rs \
            --crate-type bin \
            --emit=metadata \
            --out-dir "$case_root/target-direct"
) >"$case_root/direct.stdout" 2>"$case_root/direct.stderr"
direct_status=$?
(
    cd "$case_workspace"
    env \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
        CARGO_PRIMARY_PACKAGE=1 \
        CARGO_CRATE_NAME=t76_standard_commands \
        CARGO_MANIFEST_DIR="$case_workspace" \
        CARGO_MANIFEST_PATH="$case_workspace/Cargo.toml" \
        ANTDELIRIUM_LIMITED_SOCKET="$case_root/unavailable.sock" \
        "$release_binary" "$rustc_bin" \
            --crate-name t76_standard_commands \
            --edition=2021 \
            src/main.rs \
            --crate-type bin \
            --emit=metadata \
            --out-dir "$case_root/target-wrapped"
) >"$case_root/wrapped.stdout" 2>"$case_root/wrapped.stderr"
wrapped_status=$?
set -e
[[ $direct_status -ne 0 && $wrapped_status -eq $direct_status ]] \
    || fail 'the wrapper changed the compiler failure status during degradation.'
rg -q 'error' "$case_root/direct.stderr" \
    || fail 'the direct compiler failure did not contain a Rust diagnostic.'
rg -q 'error' "$case_root/wrapped.stderr" \
    || fail 'the wrapped compiler failure did not preserve a Rust diagnostic.'
if rg -q 'antdelirium\[AD-' "$case_root/wrapped.stderr"; then
    fail 'audit-only degradation replaced the compiler failure with a governance error.'
fi

wrapper_sha256=$(sha256sum "$release_binary" | cut -d ' ' -f 1)
jq -n \
    --arg wrapper_sha256 "$wrapper_sha256" \
    '{
        contract_version: "fail-open-rollout-evidence-v1",
        result: "passed",
        rollout_mode: "fail-open",
        external_project_access: "forbidden",
        wrapper_sha256: $wrapper_sha256,
        valid_observation_count: 3,
        false_positive_count: 0,
        degraded_case_count: 4,
        degraded_cases: [
            {
                case: "endpoint-absent",
                classification: "integration-absent",
                compiler_result: "preserved"
            },
            {
                case: "service-unavailable",
                classification: "service-unavailable",
                compiler_result: "preserved"
            },
            {
                case: "malformed-protocol",
                classification: "protocol",
                compiler_result: "preserved"
            },
            {
                case: "projection-materialization",
                classification: "projection-materialization",
                compiler_result: "preserved"
            }
        ],
        compiler_failure_precedence: "preserved",
        enforcement_activated: false
    }' >"$log_root/fail-open-rollout-evidence-v1.json"

jq -e '
    .contract_version == "fail-open-rollout-evidence-v1" and
    .result == "passed" and
    .rollout_mode == "fail-open" and
    .external_project_access == "forbidden" and
    (.wrapper_sha256 | test("^[0-9a-f]{64}$")) and
    .valid_observation_count == 3 and
    .false_positive_count == 0 and
    .degraded_case_count == 4 and
    (.degraded_cases | length == 4) and
    ([.degraded_cases[].case] | unique | length == 4) and
    ([.degraded_cases[].compiler_result] | all(. == "preserved")) and
    .compiler_failure_precedence == "preserved" and
    .enforcement_activated == false
' "$log_root/fail-open-rollout-evidence-v1.json" >/dev/null \
    || fail 'the fail-open rollout evidence violates its closed contract.'

printf '%s\n' 'AD-T91: fail-open rollout passed with zero false positives across three valid observations.'
