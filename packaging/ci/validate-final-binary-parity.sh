#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
release_binary="$repository_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
log_root="$repository_root/target/t101-final-binary-parity-logs/$run_id"
cargo_bin=$(command -v cargo || true)

fail() {
    printf '%s\n' "AD-T101-PARITY: $1" >&2
    exit 1
}

for required_command in cargo jq rg bash; do
    command -v "$required_command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $required_command"
done
[[ -x "$release_binary" && ! -L "$release_binary" ]] \
    || fail 'the reviewed release binary is unavailable.'

umask 077
mkdir -p "$log_root"

printf '%s\n' 'AD-T101-PARITY: proving the single physical Cargo target.'
(cd "$repository_root" && "$cargo_bin" metadata --locked --offline --no-deps --format-version 1) \
    >"$log_root/cargo-metadata.json"
jq -e '
    [.packages[] | select(.name == "ac_ant_delirium") | .targets[] |
      select(.kind | index("bin")) | .name] == ["antdelirium"]
' "$log_root/cargo-metadata.json" >/dev/null \
    || fail 'Cargo metadata does not expose exactly one authoritative binary.'

"$release_binary" --version >"$log_root/version.out" 2>"$log_root/version.err" \
    || fail 'the authoritative version surface failed.'
"$release_binary" --help >"$log_root/help.out" 2>"$log_root/help.err" \
    || fail 'the authoritative agent help failed.'
[[ ! -s "$log_root/version.err" && ! -s "$log_root/help.err" ]] \
    || fail 'a successful public probe wrote to stderr.'
rg -F -q -- 'antdelirium --api hierarchy.task.current' "$log_root/help.out" \
    || fail 'the current-task surface is absent from the authoritative help.'
if rg -q 'delirium_cli|ac_ant_delirium|(^|[[:space:]])(\./)?guard([[:space:]]|$)|--project-root|--config' \
    "$log_root/help.out" "$log_root/version.out"; then
    fail 'a public probe exposed a legacy executable or selector.'
fi

run_exact_test() {
    local test_name=$1
    local log_name=$2
    (cd "$repository_root" && \
        "$cargo_bin" test --locked --offline \
            --target x86_64-unknown-linux-gnu \
            --bin antdelirium \
            "$test_name" -- --exact) \
        >"$log_root/$log_name.stdout" 2>"$log_root/$log_name.stderr" \
        || fail "the focused parity test failed: $log_name"
    rg -F -q -- 'test result: ok.' "$log_root/$log_name.stdout" \
        || fail "the focused parity test did not report success: $log_name"
}

printf '%s\n' 'AD-T101-PARITY: exercising the six capability-scoped API operations.'
run_exact_test \
    'agent_cli::tests::task_and_rule_queries_return_only_local_projection_notifications' \
    'task-and-rule-read'
run_exact_test \
    'agent_cli::tests::next_action_prioritizes_governance_then_current_task' \
    'next-action'
run_exact_test \
    'agent_cli::tests::submission_builds_a_typed_request_without_writing_project_state' \
    'submission-create'
run_exact_test \
    'reviewer_cli::tests::reviewer_parser_is_closed_to_one_assignment_or_decision' \
    'reviewer-surface'
run_exact_test \
    'limited_protocol::limited_protocol_tests::agent_submission_and_assigned_review_are_scoped_immutable_and_audited_over_adp1' \
    'submission-read-and-review'

printf '%s\n' 'AD-T101-PARITY: proving explicit audit through unchanged Cargo commands.'
bash "$repository_root/packaging/cargo/validate-standard-commands.sh" \
    >"$log_root/standard-cargo.stdout" 2>"$log_root/standard-cargo.stderr" \
    || {
        sed -n '1,120p' "$log_root/standard-cargo.stderr" >&2
        fail 'the ordinary Cargo command matrix failed.'
    }
rg -F -q -- \
    'AD-T76: cargo check, build, test, run and clippy passed through the installed integration.' \
    "$log_root/standard-cargo.stdout" \
    || fail 'the ordinary Cargo integration did not produce its parity receipt.'

cat >"$log_root/evidence.json" <<'EOF'
{
  "contract_version": "final-binary-parity-evidence-v1",
  "physical_binary_count": 1,
  "authoritative_binary": "antdelirium",
  "legacy_executable_targets": 0,
  "surviving_capability_count": 7,
  "current_task_read": "passed",
  "related_rule_read": "passed",
  "next_action_read": "passed",
  "submission_create": "passed",
  "submission_read_assigned": "passed",
  "review_decide_assigned": "passed",
  "explicit_audit": "ordinary-cargo-check-passed",
  "legacy_mutation_parity": "forbidden",
  "external_project_access": "forbidden",
  "validation": "pass"
}
EOF

printf '%s\n' 'AD-T101-PARITY: seven approved capabilities are covered by the single antdelirium executable.'
printf '%s\n' "AD-T101-PARITY: logs=$log_root"
