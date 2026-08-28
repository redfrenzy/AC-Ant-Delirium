#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t76-standard-cargo"
release_binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
evidence_schema="$project_root/docs/contracts/v1/bypass-matrix-evidence-v1.schema.json"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t92-bypass-matrix/$run_id"
cargo_bin=$(command -v cargo || true)
rustc_bin=

fail() {
    printf '%s\n' "AD-T92: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=antdelirium-bypass-matrix-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T92: logs=$log_root"
    exit "$exit_code"
}

for command in cargo cp cut date find jq mkdir rg rustc sha256sum; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -n "$cargo_bin" ]] || fail 'Cargo is required for the bypass matrix.'
rustc_bin=$(rustc --print sysroot 2>/dev/null)/bin/rustc
[[ -x "$rustc_bin" && ! -L "$rustc_bin" ]] \
    || fail 'the resolved native rustc is unavailable.'
[[ -x "$release_binary" && ! -L "$release_binary" ]] \
    || fail 'build the reviewed native release before the bypass matrix.'
[[ -f "$fixture_source/Cargo.lock" && ! -L "$fixture_source" ]] \
    || fail 'the maintained synthetic fixture is unavailable.'
[[ -f "$evidence_schema" ]] || fail 'the bypass evidence contract is unavailable.'
jq empty "$evidence_schema" || fail 'the bypass evidence contract is invalid.'
case "$release_binary$rustc_bin" in
    *'"'*|*'\'*) fail 'tool paths cannot be represented by closed Cargo configuration.' ;;
esac

umask 077
mkdir -p "$log_root"
trap finish EXIT

source_digest() {
    local workspace=$1
    sha256sum \
        "$workspace/Cargo.toml" \
        "$workspace/Cargo.lock" \
        "$workspace/src/main.rs" \
        | cut -d ' ' -f 1 \
        | sha256sum \
        | cut -d ' ' -f 1
}

prepare_case() {
    local vector=$1
    case_root="$log_root/$vector"
    canonical_workspace="$case_root/canonical-workspace"
    bypass_workspace="$canonical_workspace"
    bypass_target="$case_root/bypass-target"
    protected_target="$case_root/protected-target"
    case_cargo_home="$case_root/untrusted-cargo-home"
    mkdir -p \
        "$canonical_workspace" \
        "$bypass_target" \
        "$protected_target" \
        "$case_cargo_home"
    cp -R "$fixture_source/." "$canonical_workspace/"
    expected_source_digest=$(source_digest "$canonical_workspace")
}

run_cargo_bypass() {
    local vector=$1
    local -a cargo_arguments=(-vv check --locked --offline)

    if [[ "$vector" == local-config ]]; then
        printf '%s\n' \
            '[build]' \
            'target = "x86_64-unknown-linux-gnu"' \
            'rustc-workspace-wrapper = ""' \
            >"$bypass_workspace/.cargo/config.toml"
    elif [[ "$vector" == cargo-config-argument ]]; then
        cargo_arguments=(
            --config 'build.rustc-workspace-wrapper=""'
            "${cargo_arguments[@]}"
        )
    fi

    if [[ "$vector" == environment || "$vector" == project-copy ]]; then
        (
            cd "$bypass_workspace"
            env \
                -u RUSTC \
                -u RUSTC_WRAPPER \
                -u CARGO_BUILD_RUSTC \
                -u CARGO_BUILD_RUSTC_WRAPPER \
                -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
                RUSTC_WORKSPACE_WRAPPER= \
                CARGO_HOME="$case_cargo_home" \
                CARGO_TARGET_DIR="$bypass_target" \
                "$cargo_bin" "${cargo_arguments[@]}"
        ) >"$case_root/bypass.stdout" 2>"$case_root/bypass.stderr"
    else
        (
            cd "$bypass_workspace"
            env \
                -u RUSTC \
                -u RUSTC_WRAPPER \
                -u RUSTC_WORKSPACE_WRAPPER \
                -u CARGO_BUILD_RUSTC \
                -u CARGO_BUILD_RUSTC_WRAPPER \
                -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER \
                CARGO_HOME="$case_cargo_home" \
                CARGO_TARGET_DIR="$bypass_target" \
                "$cargo_bin" "${cargo_arguments[@]}"
        ) >"$case_root/bypass.stdout" 2>"$case_root/bypass.stderr"
    fi

    rg -F -- 'CARGO_PRIMARY_PACKAGE=1' "$case_root/bypass.stderr" \
        >"$case_root/bypass-primary.txt" \
        || fail "$vector did not compile the primary package during the local control."
    if rg -F -q -- "$release_binary" "$case_root/bypass-primary.txt"; then
        fail "$vector did not bypass the trusted wrapper in the local control."
    fi
    rg -F -q -- "$rustc_bin" "$case_root/bypass-primary.txt" \
        || fail "$vector did not invoke the real compiler directly."
}

run_direct_rustc_bypass() {
    mkdir -p "$case_root/direct-output"
    (
        cd "$bypass_workspace"
        env \
            -u ANTDELIRIUM_LIMITED_SOCKET \
            -u ANTDELIRIUM_LIMITED_CAPABILITY_FD \
            -u ANTDELIRIUM_WRAPPER_DEPTH \
            "$rustc_bin" \
                --crate-name t76_standard_commands \
                --edition=2021 \
                src/main.rs \
                --crate-type bin \
                --emit=metadata \
                --out-dir "$case_root/direct-output"
    ) >"$case_root/bypass.stdout" 2>"$case_root/bypass.stderr"
    [[ -n $(find "$case_root/direct-output" -mindepth 1 -maxdepth 1 -type f -print -quit) ]] \
        || fail 'direct rustc did not produce its compiler artifact.'
}

run_protected_revalidation() {
    local vector=$1
    local endpoint_config="$case_root/protected-endpoint.toml"
    local trusted_rustc_config="build.rustc=\"$rustc_bin\""
    local trusted_outer_wrapper_config='build.rustc-wrapper=""'
    local trusted_workspace_wrapper_config="build.rustc-workspace-wrapper=\"$release_binary\""
    printf '[env]\nANTDELIRIUM_LIMITED_SOCKET = { value = "%s", force = true }\n' \
        "$case_root/unavailable.sock" >"$endpoint_config"

    (
        cd "$canonical_workspace"
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
            CARGO_HOME="$case_cargo_home" \
            CARGO_TARGET_DIR="$protected_target" \
            "$cargo_bin" \
                --config "$trusted_rustc_config" \
                --config "$trusted_outer_wrapper_config" \
                --config "$trusted_workspace_wrapper_config" \
                --config "$endpoint_config" \
                -vv check --locked --offline
    ) >"$case_root/protected.stdout" 2>"$case_root/protected.stderr" \
        || fail "$vector blocked the protected canonical revalidation."

    rg -F -- 'CARGO_PRIMARY_PACKAGE=1' "$case_root/protected.stderr" \
        >"$case_root/protected-primary.txt" \
        || fail "$vector did not trigger a primary protected compilation."
    rg -F -q -- "$release_binary" "$case_root/protected-primary.txt" \
        || fail "$vector escaped the trusted wrapper during protected revalidation."
    [[ $(source_digest "$canonical_workspace") == "$expected_source_digest" ]] \
        || fail "$vector changed canonical source bytes during validation."
}

local_bypass_observed_count=0
protected_revalidation_count=0
for vector in environment local-config cargo-config-argument direct-rustc project-copy; do
    printf '%s\n' "AD-T92: exercising local bypass vector: $vector"
    prepare_case "$vector"
    if [[ "$vector" == project-copy ]]; then
        bypass_workspace="$case_root/untrusted-project-copy"
        mkdir -p "$bypass_workspace"
        cp -R "$canonical_workspace/." "$bypass_workspace/"
        [[ $(source_digest "$bypass_workspace") == "$expected_source_digest" ]] \
            || fail 'the copied project did not preserve the synthetic source bytes.'
    fi
    if [[ "$vector" == direct-rustc ]]; then
        run_direct_rustc_bypass
    else
        run_cargo_bypass "$vector"
    fi
    [[ ! -e "$bypass_workspace/guard" ]] \
        || fail "$vector unexpectedly published governance projections during bypass."
    local_bypass_observed_count=$((local_bypass_observed_count + 1))

    printf '%s\n' "AD-T92: revalidating canonical source with trusted wrapper: $vector"
    run_protected_revalidation "$vector"
    protected_revalidation_count=$((protected_revalidation_count + 1))
done

[[ $local_bypass_observed_count -eq 5 && $protected_revalidation_count -eq 5 ]] \
    || fail 'the bypass matrix did not cover every closed vector.'

wrapper_sha256=$(sha256sum "$release_binary" | cut -d ' ' -f 1)
jq -n \
    --arg wrapper_sha256 "$wrapper_sha256" \
    '{
        contract_version: "bypass-matrix-evidence-v1",
        result: "passed",
        external_project_access: "forbidden",
        authority_boundary: "ci-supervisor-required",
        wrapper_sha256: $wrapper_sha256,
        case_count: 5,
        local_bypass_observed_count: 5,
        protected_revalidation_count: 5,
        cases: [
            {vector: "environment", local_result: "bypass-observed", protected_result: "trusted-wrapper-verified"},
            {vector: "local-config", local_result: "bypass-observed", protected_result: "trusted-wrapper-verified"},
            {vector: "cargo-config-argument", local_result: "bypass-observed", protected_result: "trusted-wrapper-verified"},
            {vector: "direct-rustc", local_result: "bypass-observed", protected_result: "trusted-wrapper-verified"},
            {vector: "project-copy", local_result: "bypass-observed", protected_result: "trusted-wrapper-verified"}
        ],
        copy_case: "canonical-source-revalidated",
        enforcement_activated: false
    }' >"$log_root/bypass-matrix-evidence-v1.json"

jq -e '
    .contract_version == "bypass-matrix-evidence-v1" and
    .result == "passed" and
    .external_project_access == "forbidden" and
    .authority_boundary == "ci-supervisor-required" and
    (.wrapper_sha256 | test("^[0-9a-f]{64}$")) and
    .case_count == 5 and
    .local_bypass_observed_count == 5 and
    .protected_revalidation_count == 5 and
    (.cases | length == 5) and
    ([.cases[].vector] | unique | length == 5) and
    ([.cases[].local_result] | all(. == "bypass-observed")) and
    ([.cases[].protected_result] | all(. == "trusted-wrapper-verified")) and
    .copy_case == "canonical-source-revalidated" and
    .enforcement_activated == false
' "$log_root/bypass-matrix-evidence-v1.json" >/dev/null \
    || fail 'the bypass matrix evidence violates its closed contract.'

printf '%s\n' 'AD-T92: all five local bypasses were observed and required trusted canonical revalidation.'
