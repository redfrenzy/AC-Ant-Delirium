#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t76-standard-cargo"
violation_source="$project_root/tests/fixtures/t8g-restricted-token-main.rs"
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
bundle_root="$project_root/target/t86-system-installation-bundle"
bundle_binary="$bundle_root/usr/libexec/antdelirium/antdelirium"
service_binary=/usr/libexec/antdelirium/antdelirium
cargo_home_path=${CARGO_HOME:-"$HOME/.cargo"}
installed_wrapper="$cargo_home_path/bin/antdelirium"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t8g-generic-installation-logs/$run_id"
synthetic_project="$log_root/project"
target_root="$log_root/cargo-targets"

fail() {
    printf '%s\n' "AD-T8G: $1" >&2
    exit 1
}

for command in cargo cp cut date find grep id jq mkdir seq sha256sum sleep sort stat systemctl; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done

[[ $(id -u) -ne 0 ]] || fail 'run this acceptance gate as the non-root operator.'
[[ -f "$fixture_source/Cargo.lock" && ! -L "$fixture_source" ]] \
    || fail 'the maintained synthetic Rust fixture is unavailable.'
[[ -f "$violation_source" && ! -L "$violation_source" ]] \
    || fail 'the maintained governance-violation fixture is unavailable.'
[[ ! -e "$fixture_source/guard" ]] \
    || fail 'the maintained fixture contains generated state; preserve it under target first.'
[[ -x "$release_binary" && ! -L "$release_binary" ]] \
    || fail 'the reviewed native release is unavailable.'
[[ -x "$installed_wrapper" && ! -L "$installed_wrapper" ]] \
    || fail 'the global Cargo wrapper is unavailable.'
[[ -x "$service_binary" && ! -L "$service_binary" ]] \
    || fail 'the installed service binary is unavailable.'
[[ -x "$bundle_binary" && ! -L "$bundle_binary" ]] \
    || fail 'the reviewed installation bundle is unavailable.'
[[ -f "$bundle_root/system-installation-bundle-v1.json" ]] \
    || fail 'the reviewed installation bundle manifest is unavailable.'

umask 077
mkdir -p "$log_root" "$target_root"
cp -R "$fixture_source" "$synthetic_project"

finish() {
    local exit_code=$?
    {
        printf 'contract=antdelirium-installed-generic-acceptance-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T8G: logs=$log_root"
}
trap finish EXIT

release_sha256=$(sha256sum "$release_binary" | cut -d ' ' -f 1)
wrapper_sha256=$(sha256sum "$installed_wrapper" | cut -d ' ' -f 1)
service_sha256=$(sha256sum "$service_binary" | cut -d ' ' -f 1)
bundle_sha256=$(sha256sum "$bundle_binary" | cut -d ' ' -f 1)
manifest_sha256=$(jq -r '
    .files[] | select(.artifact == "service-binary") | .sha256
' "$bundle_root/system-installation-bundle-v1.json")

[[ "$release_sha256" == "$wrapper_sha256" ]] \
    || fail 'the global Cargo wrapper does not match the reviewed release.'
[[ "$release_sha256" == "$service_sha256" ]] \
    || fail 'the installed service does not match the reviewed release.'
[[ "$release_sha256" == "$bundle_sha256" ]] \
    || fail 'the installation bundle does not match the reviewed release.'
[[ "$release_sha256" == "$manifest_sha256" ]] \
    || fail 'the bundle manifest does not attest the reviewed release.'

if ! (
    cd "$synthetic_project"
    env \
        -u RUSTC \
        -u RUSTC_WRAPPER \
        -u RUSTC_WORKSPACE_WRAPPER \
        -u CARGO_BUILD_RUSTC \
        -u CARGO_BUILD_RUSTC_WRAPPER \
        -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
        -u ANTDELIRIUM_WRAPPER_DEPTH \
        "$installed_wrapper" doctor
) >"$log_root/doctor.stdout" 2>"$log_root/doctor.stderr"; then
    fail 'doctor did not report a ready installed integration.'
fi
jq -Rn '[inputs] | any(. == "result=ready")' <"$log_root/doctor.stdout" \
    | jq -e '. == true' >/dev/null \
    || fail 'doctor output did not contain result=ready.'

for unit in antdelirium.service antdelirium-agent.socket antdelirium-admin.socket; do
    [[ $(systemctl is-active "$unit" 2>/dev/null || true) == active ]] \
        || fail 'the installed service and activation sockets must be active.'
done

run_cargo_command() {
    local command_name=$1
    local audit_diagnostic="$log_root/cargo-$command_name.audit"
    if ! (
        cd "$synthetic_project"
        env \
            -u RUSTC \
            -u RUSTC_WRAPPER \
            -u RUSTC_WORKSPACE_WRAPPER \
            -u CARGO_BUILD_RUSTC \
            -u CARGO_BUILD_RUSTC_WRAPPER \
            -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
            -u ANTDELIRIUM_WRAPPER_DEPTH \
            ANTDELIRIUM_SUPERVISOR_DIAGNOSTIC_FD=3 \
            CARGO_TARGET_DIR="$target_root/$command_name" \
            cargo "$command_name" 3>"$audit_diagnostic"
    ) >"$log_root/cargo-$command_name.stdout" \
        2>"$log_root/cargo-$command_name.stderr"; then
        fail "cargo $command_name failed through the installed integration."
    fi
}

require_audit_pass() {
    local command_name=$1
    local audit_diagnostic="$log_root/cargo-$command_name.audit"
    for _ in $(seq 1 100); do
        [[ -s "$audit_diagnostic" ]] && break
        sleep 0.1
    done
    if ! grep -Fxq 'audit=pass' "$audit_diagnostic"; then
        printf '%s\n' 'AD-T8G: wrapper audit diagnostic:' >&2
        sed -n '1,40p' "$audit_diagnostic" >&2
        fail "cargo $command_name did not complete the protected audit."
    fi
}

cp "$violation_source" "$synthetic_project/src/main.rs"
run_cargo_command check
require_audit_pass check

guard_root="$synthetic_project/guard"
todo_projection="$guard_root/to-do_list.json"
error_projection="$guard_root/governance_error.json"
for _ in $(seq 1 100); do
    if [[ -f "$error_projection" ]] && jq -e '
        any(.violations[]?;
            (.rule_id | contains("restricted_tokens")) and
            .path == "src/main.rs")
    ' "$error_projection" >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done
jq -e '
    any(.violations[]?;
        (.rule_id | contains("restricted_tokens")) and
        .path == "src/main.rs")
' "$error_projection" >/dev/null \
    || fail 'cargo check did not project the restricted-token violation.'
cp "$error_projection" "$log_root/governance-error-with-violation.json"

cp "$fixture_source/src/main.rs" "$synthetic_project/src/main.rs"
run_cargo_command build
require_audit_pass build
for _ in $(seq 1 100); do
    if [[ -f "$error_projection" ]] \
        && jq -e '
            all(.violations[]?;
                (.rule_id | contains("restricted_tokens") | not) and
                (.rule_id | contains("token_sanitization") | not))
        ' "$error_projection" >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done
jq -e '
    all(.violations[]?;
        (.rule_id | contains("restricted_tokens") | not) and
        (.rule_id | contains("token_sanitization") | not))
' "$error_projection" >/dev/null \
    || fail 'the corrected source did not clear the introduced token violation.'
run_cargo_command test
require_audit_pass test

[[ -d "$guard_root" && ! -L "$guard_root" ]] \
    || fail 'normal Cargo commands did not materialize the public guard directory.'

mapfile -t guard_entries < <(find "$guard_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)
[[ ${#guard_entries[@]} -eq 2 \
    && ${guard_entries[0]} == governance_error.json \
    && ${guard_entries[1]} == to-do_list.json ]] \
    || fail 'the project guard directory contains something other than the two public projections.'

for projection in "$todo_projection" "$error_projection"; do
    [[ -f "$projection" && ! -L "$projection" ]] \
        || fail 'a public projection is not a regular file.'
    [[ $(stat -c '%a' "$projection") == 600 ]] \
        || fail 'public projections must use mode 0600.'
    jq empty "$projection" || fail 'a public projection is not valid JSON.'
    if LC_ALL=C grep -Eq '/home/|/Users/|[A-Za-z]:\\\\Users\\\\|/var/|/run/|/tmp/' "$projection"; then
        fail 'a public projection disclosed a physical locator.'
    fi
done

jq -e '
    .contract_version == "agent-projection-v1" and
    .artifact_kind == "current_task" and
    (.project_ref | test("^prj_[0-9a-z]{26}$")) and
    (.workspace_ref | test("^wsp_[0-9a-z]{26}$")) and
    (.source_revision | type == "number") and
    .task == null and
    .queue_status == "no_task_available" and
    .submission.contract_version == "submission-v1"
' "$todo_projection" >/dev/null \
    || fail 'the current-task projection violates its public contract.'

jq -e '
    .contract_version == "agent-projection-v1" and
    .artifact_kind == "governance_errors" and
    (.project_ref | test("^prj_[0-9a-z]{26}$")) and
    (.workspace_ref | test("^wsp_[0-9a-z]{26}$")) and
    (.source_revision | type == "number") and
    (.violations | type == "array") and
    all(.violations[]?;
        (.rule_id | contains("restricted_tokens") | not) and
        (.rule_id | contains("token_sanitization") | not))
' "$error_projection" >/dev/null \
    || fail 'the governance-error projection violates its public contract.'

[[ $(jq -r '.project_ref' "$todo_projection") == \
    "$(jq -r '.project_ref' "$error_projection")" ]] \
    || fail 'the public projections disagree about the opaque project reference.'
[[ $(jq -r '.workspace_ref' "$todo_projection") == \
    "$(jq -r '.workspace_ref' "$error_projection")" ]] \
    || fail 'the public projections disagree about the opaque workspace reference.'

for obsolete in login.json AC_Ant-Delirium_props plugins; do
    [[ ! -e "$synthetic_project/$obsolete" ]] \
        || fail 'normal Cargo commands created obsolete project-local state.'
done

{
    printf 'contract=antdelirium-installed-generic-evidence-v1\n'
    printf 'physical_locators=redacted\n'
    printf 'external_project_access=forbidden\n'
    printf 'release_sha256=%s\n' "$release_sha256"
    printf 'wrapper_matches_release=yes\n'
    printf 'service_matches_release=yes\n'
    printf 'bundle_matches_release=yes\n'
    printf 'doctor=ready\n'
    printf 'cargo_check=pass\n'
    printf 'cargo_check_violation_projection=pass\n'
    printf 'corrected_source_token_violation=cleared\n'
    printf 'cargo_build=pass\n'
    printf 'cargo_test=pass\n'
    printf 'public_projection_count=2\n'
    printf 'public_projection_contract=pass\n'
    printf 'physical_locator_disclosure=none\n'
} >"$log_root/acceptance.env"

printf '%s\n' 'AD-T8G: installed binary, Cargo integration, service and synthetic project acceptance passed.'
