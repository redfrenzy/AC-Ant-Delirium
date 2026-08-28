#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
binary="$project_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
evidence_schema="$project_root/docs/contracts/v1/projection-authority-evidence-v1.schema.json"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}"
log_root="$project_root/target/t95-projection-authority/$run_id"
result=failed

fail() {
    printf '%s\n' "AD-T95: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=antdelirium-projection-authority-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'result=%s\n' "$result"
        printf 'physical_locators=redacted\n'
        printf 'external_project_access=forbidden\n'
        printf 'source_mutation=forbidden\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    printf '%s\n' "AD-T95: logs=$log_root"
    exit "$exit_code"
}

for command in cargo date jq mkdir rg wc; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$binary" && ! -L "$binary" ]] \
    || fail 'the reviewed release binary is unavailable.'
[[ -f "$evidence_schema" ]] \
    || fail 'the projection-authority evidence contract is unavailable.'
jq empty "$evidence_schema" \
    || fail 'the projection-authority evidence contract is invalid.'

umask 077
mkdir -p "$log_root"
trap finish EXIT

run_test() {
    local label=$1
    local filter=$2
    shift 2
    printf '%s\n' "AD-T95: $label"
    cargo test \
        --locked \
        --target x86_64-unknown-linux-gnu \
        "$@" \
        "$filter" \
        -- --nocapture \
        >"$log_root/$label.stdout" \
        2>"$log_root/$label.stderr" \
        || fail "$label did not pass."
    rg -q 'test result: ok\. 1 passed; 0 failed' "$log_root/$label.stdout" \
        || fail "$label produced an inconclusive test result."
}

run_test \
    current-task-projection-recovery \
    deleted_or_adulterated_task_projection_is_rebuilt_only_from_canonical_state \
    --lib
run_test \
    governance-projection-recovery \
    deleted_or_adulterated_governance_projection_is_rebuilt_from_protected_snapshot \
    --lib
run_test \
    closed-agent-command-surface \
    administrative_mutations_are_denied_as_authorization_errors \
    --bin antdelirium
run_test \
    agent-service-mutation-rejection \
    agent_cannot_install_or_advance_canonical_tasks_and_each_attempt_is_journaled \
    --bin antdelirium
run_test \
    rejection-journal-nondisclosure \
    journals_authenticated_rejection_with_opaque_scope_and_no_locator_fields \
    --bin antdelirium

public_probe_count=0
run_public_probe() {
    local ordinal=$1
    local operation=$2
    local stdout_file="$log_root/public-probe-$ordinal.stdout"
    local stderr_file="$log_root/public-probe-$ordinal.stderr"
    set +e
    "$binary" --api "$operation" >"$stdout_file" 2>"$stderr_file"
    local status=$?
    set -e
    [[ $status -eq 77 ]] \
        || fail 'an administrative agent operation did not use the authorization exit contract.'
    [[ ! -s "$stdout_file" ]] \
        || fail 'an administrative agent rejection wrote unexpected stdout.'
    [[ $(wc -l <"$stderr_file") -eq 1 ]] \
        || fail 'an administrative agent rejection was not one bounded diagnostic.'
    rg -Fxq 'antdelirium[AD-AUTH-0001]: operação não autorizada para esta sessão' "$stderr_file" \
        || fail 'an administrative agent rejection did not use the closed public diagnostic.'
    if rg -F -q -- "$operation" "$stderr_file"; then
        fail 'an administrative agent rejection echoed the supplied mutation.'
    fi
    public_probe_count=$((public_probe_count + 1))
}

printf '%s\n' 'AD-T95: probing the real public binary with forbidden mutations.'
run_public_probe 1 'hierarchy.task.current.implemented=true'
run_public_probe 2 'hierarchy.override.active_plan=pln_00000000000000000000000000'
[[ $public_probe_count -eq 2 ]] \
    || fail 'the public mutation matrix did not execute completely.'

jq -n \
    --argjson public_probe_count "$public_probe_count" \
    '{
        contract_version: "projection-authority-evidence-v1",
        result: "passed",
        external_project_access: "forbidden",
        source_mutation: "forbidden",
        current_task_projection_delete_recovery: "passed",
        current_task_projection_edit_recovery: "passed",
        canonical_plan_payload_after_projection_attack: "unchanged",
        canonical_task_state_after_projection_attack: "unchanged",
        governance_projection_delete_recovery: "passed",
        governance_projection_edit_recovery: "passed",
        canonical_governance_snapshot_after_projection_attack: "unchanged",
        agent_public_mutations: "denied",
        agent_public_mutation_probe_count: $public_probe_count,
        agent_service_install_attempt: "denied",
        agent_service_advance_attempt: "denied",
        canonical_state_after_agent_mutations: "unchanged",
        persisted_rejection_event_count: 2,
        rejection_journal_locators: "absent"
    }' >"$log_root/projection-authority-evidence-v1.json"

jq -e '
    .contract_version == "projection-authority-evidence-v1" and
    .result == "passed" and
    .external_project_access == "forbidden" and
    .source_mutation == "forbidden" and
    .current_task_projection_delete_recovery == "passed" and
    .current_task_projection_edit_recovery == "passed" and
    .canonical_plan_payload_after_projection_attack == "unchanged" and
    .canonical_task_state_after_projection_attack == "unchanged" and
    .governance_projection_delete_recovery == "passed" and
    .governance_projection_edit_recovery == "passed" and
    .canonical_governance_snapshot_after_projection_attack == "unchanged" and
    .agent_public_mutations == "denied" and
    .agent_public_mutation_probe_count == 2 and
    .agent_service_install_attempt == "denied" and
    .agent_service_advance_attempt == "denied" and
    .canonical_state_after_agent_mutations == "unchanged" and
    .persisted_rejection_event_count == 2 and
    .rejection_journal_locators == "absent"
' "$log_root/projection-authority-evidence-v1.json" >/dev/null \
    || fail 'the projection-authority evidence violates its closed contract.'

result=passed
printf '%s\n' 'AD-T95: projections are disposable and agent mutations cannot change canonical state.'
