#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
fixture_source="$project_root/tests/fixtures/t77-authoritative-ci"
evidence_schema="$project_root/docs/contracts/v1/ci-gate-evidence-v1.schema.json"
default_wrapper="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
trusted_wrapper=${ANTDELIRIUM_TRUSTED_WRAPPER:-$default_wrapper}
expected_wrapper_sha256=${ANTDELIRIUM_EXPECTED_WRAPPER_SHA256:-}
probe_id="${BASHPID:-$$}-${RANDOM}"
probe_root="$project_root/target/t77-authoritative-ci/$probe_id"
workspace="$probe_root/workspace"
untrusted_cargo_home="$probe_root/untrusted-cargo-home"
clean_target="$probe_root/clean-target"
proof_target="$probe_root/wrapper-proof-target"
control_target="$probe_root/unprotected-control-target"
cargo_bin=$(command -v cargo || true)
rustc_bin=$(command -v rustc || true)

fail() {
    printf '%s\n' "AD-T77: $1" >&2
    exit 1
}

for required in "$cargo_bin" "$rustc_bin"; do
    [[ -n "$required" ]] || {
        printf '%s\n' 'AD-T77: Cargo and rustc are required for the authoritative gate.' >&2
        exit 77
    }
done
for required_command in jq rg sha256sum; do
    command -v "$required_command" >/dev/null 2>&1 || {
        printf '%s\n' "AD-T77: $required_command is required for the authoritative gate." >&2
        exit 77
    }
done
[[ -x "$trusted_wrapper" ]] || fail 'the trusted release wrapper is unavailable.'
[[ -r "$fixture_source/Cargo.lock" ]] || fail 'the locked CI fixture is unavailable.'
[[ -r "$evidence_schema" ]] || fail 'the CI evidence schema is unavailable.'
jq empty "$evidence_schema" || fail 'the CI evidence schema is invalid JSON.'
case "$trusted_wrapper$rustc_bin" in
    *'"'*|*'\\'*) fail 'tool paths cannot be represented by the closed Cargo config arguments.' ;;
esac

wrapper_sha256=$(sha256sum "$trusted_wrapper" | cut -d ' ' -f 1)
authority_mode=local-mechanics
if [[ -n "$expected_wrapper_sha256" ]]; then
    [[ "$expected_wrapper_sha256" =~ ^[0-9a-f]{64}$ ]] \
        || fail 'the protected wrapper digest has an invalid shape.'
    [[ "$wrapper_sha256" == "$expected_wrapper_sha256" ]] \
        || fail 'the trusted wrapper digest does not match protected policy.'
    authority_mode=protected-digest
fi

umask 077
mkdir -p \
    "$workspace" \
    "$untrusted_cargo_home/bin" \
    "$clean_target" \
    "$proof_target" \
    "$control_target"
cp -R "$fixture_source/." "$workspace/"
cp /bin/false "$untrusted_cargo_home/bin/antdelirium"
chmod 0755 "$untrusted_cargo_home/bin/antdelirium"
printf '%s\n' \
    '[build]' \
    'rustc-workspace-wrapper = "/bin/false"' \
    >"$untrusted_cargo_home/config.toml"

trusted_rustc_config="build.rustc=\"$rustc_bin\""
trusted_outer_wrapper_config='build.rustc-wrapper=""'
trusted_workspace_wrapper_config="build.rustc-workspace-wrapper=\"$trusted_wrapper\""

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
        CARGO_HOME="$untrusted_cargo_home" \
        CARGO_TARGET_DIR="$control_target" \
        "$cargo_bin" check --locked --offline
) >"$probe_root/control.stdout" 2>"$probe_root/control.stderr"
control_status=$?
set -e
[[ $control_status -ne 0 ]] \
    || fail 'the deliberately divergent local integration did not establish the control failure.'

run_protected_check() {
    local target_dir=$1
    local stdout_file=$2
    local stderr_file=$3
    local depth=${4:-}
    local -a environment=(
        -u RUSTC
        -u RUSTC_WRAPPER
        -u RUSTC_WORKSPACE_WRAPPER
        -u CARGO_BUILD_RUSTC
        -u CARGO_BUILD_RUSTC_WRAPPER
        -u CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER
        -u ANTDELIRIUM_WRAPPER_DEPTH
        -u ANTDELIRIUM_LIMITED_SOCKET
        -u ANTDELIRIUM_LIMITED_CAPABILITY_FD
        CARGO_HOME="$untrusted_cargo_home"
        CARGO_TARGET_DIR="$target_dir"
    )
    if [[ -n "$depth" ]]; then
        environment+=(ANTDELIRIUM_WRAPPER_DEPTH="$depth")
    fi

    (
        cd "$workspace"
        env "${environment[@]}" \
            "$cargo_bin" \
            --config "$trusted_rustc_config" \
            --config "$trusted_outer_wrapper_config" \
            --config "$trusted_workspace_wrapper_config" \
            -vv \
            check \
            --locked \
            --offline
    ) >"$stdout_file" 2>"$stderr_file"
}

set +e
run_protected_check \
    "$proof_target" \
    "$probe_root/proof.stdout" \
    "$probe_root/proof.stderr" \
    invalid
proof_status=$?
set -e
[[ $proof_status -ne 0 ]] || fail 'the trusted wrapper participation probe unexpectedly passed.'
rg -F -q -- 'AD-CONFIG-0001' "$probe_root/proof.stderr" \
    || fail 'the trusted wrapper did not produce its closed proof error.'

run_protected_check \
    "$clean_target" \
    "$probe_root/clean.stdout" \
    "$probe_root/clean.stderr" \
    || fail 'the clean authoritative check failed.'
rg -F -- "$trusted_wrapper" "$probe_root/clean.stderr" \
    >"$probe_root/clean-wrapper-lines.txt" \
    || fail 'the clean check did not prove a primary compilation through the trusted wrapper.'
rg -F -q -- 'CARGO_PRIMARY_PACKAGE=1' "$probe_root/clean-wrapper-lines.txt" \
    || fail 'the trusted wrapper invocation was not a primary package compilation.'
rg -F -q -- '--crate-name t77_authoritative_ci' "$probe_root/clean.stderr" \
    || fail 'the clean check did not compile the expected primary crate.'

run_protected_check \
    "$clean_target" \
    "$probe_root/cached.stdout" \
    "$probe_root/cached.stderr" \
    || fail 'the cached follow-up check failed.'
rg -F -- "$trusted_wrapper" "$probe_root/cached.stderr" \
    >"$probe_root/cached-wrapper-lines.txt" || true
if rg -F -q -- 'CARGO_PRIMARY_PACKAGE=1' "$probe_root/cached-wrapper-lines.txt"; then
    fail 'the cached follow-up unexpectedly recompiled the primary package.'
fi

[[ ! -e "$workspace/target" ]] || fail 'the gate used a project-local target directory.'
[[ ! -e "$workspace/guard" ]] || fail 'the unavailable service created local governance state.'
[[ ! -e "$workspace/login.json" ]] || fail 'the gate created obsolete administrative state.'

wrapper_sha256_after=$(sha256sum "$trusted_wrapper" | cut -d ' ' -f 1)
[[ "$wrapper_sha256_after" == "$wrapper_sha256" ]] \
    || fail 'the trusted wrapper changed while the gate was running.'

source_sha256=$(
    sha256sum \
        "$workspace/Cargo.toml" \
        "$workspace/Cargo.lock" \
        "$workspace/.cargo/config.toml" \
        "$workspace/src/main.rs" \
        | sha256sum \
        | cut -d ' ' -f 1
)
cat >"$probe_root/ci-gate-evidence-v1.json" <<EOF
{
  "schema_version": "ci-gate-evidence-v1",
  "result": "passed",
  "authority_mode": "$authority_mode",
  "wrapper_sha256": "$wrapper_sha256",
  "source_sha256": "$source_sha256",
  "clean_target": true,
  "primary_wrapper_participation": "verified",
  "local_wrapper_override": "superseded",
  "cached_followup": "passed-without-primary-recompile",
  "signature": "none-local-mechanics-only"
}
EOF
jq -e \
    '.schema_version == "ci-gate-evidence-v1"
     and .result == "passed"
     and (.authority_mode == "local-mechanics" or .authority_mode == "protected-digest")
     and (.wrapper_sha256 | test("^[0-9a-f]{64}$"))
     and (.source_sha256 | test("^[0-9a-f]{64}$"))
     and .clean_target
     and .primary_wrapper_participation == "verified"
     and .local_wrapper_override == "superseded"
     and .cached_followup == "passed-without-primary-recompile"
     and .signature == "none-local-mechanics-only"' \
    "$probe_root/ci-gate-evidence-v1.json" >/dev/null \
    || fail 'the mechanical evidence document is invalid.'

printf '%s\n' 'AD-T77: clean CI compilation used the trusted wrapper; cached follow-up was advisory.'
