#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
state_credential=/var/lib/antdelirium-operator/state.cred
admin_socket=/run/antdelirium-admin/admin.sock
readiness_probe="$script_dir/probe-installed-service-ready.py"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
log_root="$repository_root/target/t8g-task-cycle-logs/$run_id"
secret_root="/run/antdelirium-task-cycle-$run_id"
state_plain=
package_plain=
state_fd=
package_fd=

fail() {
    printf '%s\n' "AD-T8G-CYCLE: $1" >&2
    exit 1
}

close_fd() {
    local descriptor=$1
    local direction=$2
    [[ -z "$descriptor" ]] || eval "exec ${descriptor}${direction}"
}

cleanup() {
    local exit_code=$?
    set +e
    close_fd "$state_fd" '<&-'
    close_fd "$package_fd" '<&-'
    [[ -z "$state_plain" ]] || rm -f -- "$state_plain"
    [[ -z "$package_plain" ]] || rm -f -- "$package_plain"
    rmdir -- "$secret_root" 2>/dev/null || true
    {
        printf 'contract=antdelirium-installed-task-cycle-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'external_project_access=forbidden\n'
        printf 'physical_locators=redacted\n'
        printf 'protected_plan_access=operator-descriptor-only\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T8G-CYCLE: logs=$log_root"
}

[[ $EUID -eq 0 ]] || fail 'execute this validation routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator and reviewer.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")

[[ ${1:-} == '--project-root' && -n ${2:-} ]] \
    || fail 'use --project-root <absolute-synthetic-project> [--resume-task-id <task-id>].'
target_project=$2
resume_task_id=
if [[ $# -eq 4 && ${3:-} == '--resume-task-id' ]]; then
    resume_task_id=$4
    [[ "$resume_task_id" =~ ^tsk_[0-9a-hjkmnp-tv-z]{26}$ ]] \
        || fail 'the resume task identifier is invalid.'
elif [[ $# -ne 2 ]]; then
    fail 'use --project-root <absolute-synthetic-project> [--resume-task-id <task-id>].'
fi
[[ "$target_project" == "$repository_root"/target/* && -d "$target_project" && ! -L "$target_project" ]] \
    || fail 'the project must be a non-symlink synthetic directory under this repository target.'
[[ ! -e "$target_project/.git" && ! -L "$target_project/.git" ]] \
    || fail 'the private-backup proof requires a synthetic project without project-local Git metadata.'

for command in systemd-creds setpriv jq install id tee od tr getent cut sed tail awk; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
invoking_home=$(getent passwd "$invoking_uid" | cut -d: -f6)
cargo_binary="$invoking_home/.cargo/bin/cargo"
[[ -x "$cargo_binary" ]] || fail 'the invoking user Cargo executable is unavailable.'
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed AntDelirium binary is unavailable.'
[[ -S "$admin_socket" ]] || fail 'the installed administrative socket is unavailable.'
[[ -f "$readiness_probe" && ! -L "$readiness_probe" ]] \
    || fail 'the reviewed service readiness probe is unavailable.'
python3 -B "$readiness_probe" "$admin_socket" \
    || fail 'the installed service did not complete its administrative protocol startup.'
if [[ -z "$resume_task_id" ]]; then
    [[ -f "$state_credential" && ! -L "$state_credential" ]] \
        || fail 'the encrypted StateAdmin descriptor is unavailable.'
fi

projection="$target_project/guard/to-do_list.json"
[[ -f "$projection" && ! -L "$projection" ]] \
    || fail 'the synthetic project projection is unavailable.'
if [[ -z "$resume_task_id" ]]; then
    jq -e '
        .contract_version == "agent-projection-v1" and
        .artifact_kind == "current_task" and
        (.project_ref | test("^prj_[0-9a-hjkmnp-tv-z]{26}$")) and
        (.workspace_ref | test("^wsp_[0-9a-hjkmnp-tv-z]{26}$")) and
        .source_revision == 0 and .task == null and
        .queue_status == "no_task_available"
    ' "$projection" >/dev/null || fail 'the synthetic project is not an empty canonical fixture.'
else
    jq -e --arg task "$resume_task_id" '
        .contract_version == "agent-projection-v1" and
        .artifact_kind == "current_task" and
        (.project_ref | test("^prj_[0-9a-hjkmnp-tv-z]{26}$")) and
        (.workspace_ref | test("^wsp_[0-9a-hjkmnp-tv-z]{26}$")) and
        ((.source_revision == 0 and .task == null) or
         (.source_revision == 1 and .task.task_id == $task))
    ' "$projection" >/dev/null || fail 'the synthetic project cannot resume the requested task.'
fi
project_id=$(jq -r '.project_ref' "$projection")
workspace_id=$(jq -r '.workspace_ref' "$projection")
plan_id="pln_$(od -An -N13 -tx1 /dev/urandom | tr -d ' \n')"
if [[ -n "$resume_task_id" ]]; then
    task_id=$resume_task_id
else
    task_id="tsk_$(od -An -N13 -tx1 /dev/urandom | tr -d ' \n')"
fi

umask 077
install -d -m 0755 "$log_root"
install -d -m 0700 "$secret_root"
exec > >(tee -a "$log_root/supervisor.log") 2>&1
trap cleanup EXIT

if [[ -z "$resume_task_id" ]]; then
    state_plain="$secret_root/state.json"
    systemd-creds --name=antdelirium-operator-state-v1 decrypt \
        "$state_credential" "$state_plain" >/dev/null
    chmod 0600 "$state_plain"
    jq -e '
        .contract_version == "admin-capability-v1" and
        (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
        (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
    ' "$state_plain" >/dev/null || fail 'the StateAdmin descriptor is invalid.'

    package_plain="$secret_root/task-package.json"
    jq -n \
        --arg project "$project_id" \
        --arg workspace "$workspace_id" \
        --arg plan "$plan_id" \
        --arg task "$task_id" \
        '{
            contract_version: "governance-task-package-v1",
            project_id: $project,
            workspace_id: $workspace,
            plan_id: $plan,
            expected_state_revision: 0,
            tasks: [{
                task_id: $task,
                task_revision: 1,
                objective: "Validate the installed synthetic governance cycle.",
                acceptance_criteria: [
                    "A scoped agent submission is recorded.",
                    "An assigned reviewer reads and accepts the submission.",
                    "A separately authorized CI transition completes the canonical task."
                ],
                relevant_paths: ["src/main.rs"]
            }]
        }' >"$package_plain"
    chmod 0600 "$package_plain"

    exec {state_fd}<"$state_plain"
    exec {package_fd}<"$package_plain"
    printf '%s\n' 'AD-T8G-CYCLE: installing one synthetic canonical task.'
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$state_fd" \
        ANTDELIRIUM_TASK_PACKAGE_FD="$package_fd" \
        "$installed_binary" operator task-package install
    close_fd "$state_fd" '<&-'
    close_fd "$package_fd" '<&-'
    state_fd=
    package_fd=
else
    printf '%s\n' 'AD-T8G-CYCLE: resuming after the previously installed synthetic task.'
fi

source_evidence="$target_project/src/main.rs"
[[ -f "$source_evidence" && ! -L "$source_evidence" ]] \
    || fail 'the maintained synthetic Rust source is unavailable.'

printf '%s\n' 'AD-T8G-CYCLE: refreshing public projections through an ordinary cargo check.'
install -d -m 0700 -o "$invoking_uid" -g "$invoking_gid" "$log_root/cargo-target"
(
    cd "$target_project"
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        HOME="$invoking_home" \
        CARGO_HOME="$invoking_home/.cargo" \
        CARGO_TARGET_DIR="$log_root/cargo-target" \
        "$cargo_binary" check
)

projection_matches_task() {
    jq -e --arg task "$task_id" '
        .source_revision == 1 and .task.task_id == $task and
        .task.task_revision == 1 and .queue_status == "task_available"
    ' "$projection" >/dev/null 2>&1
}

for _attempt in {1..100}; do
    projection_matches_task && break
    sleep 0.05
done
projection_matches_task \
    || fail 'cargo check did not refresh the installed canonical task within five seconds.'

printf '%s\n' 'AD-T8G-CYCLE: changing and compiling the maintained fixture after its private baseline.'
printf '\n// AntDelirium installed private-backup proof: %s\n' "$run_id" \
    | setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        tee -a "$source_evidence" >/dev/null
(
    cd "$target_project"
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        HOME="$invoking_home" \
        CARGO_HOME="$invoking_home/.cargo" \
        CARGO_TARGET_DIR="$log_root/cargo-target" \
        "$cargo_binary" check
)

printf '%s\n' 'AD-T8G-CYCLE: submitting evidence through the unchanged agent API.'
agent_output=$(bash "$script_dir/run-agent-submission.sh" \
    --project-root "$target_project" -- \
    --api 'hierarchy.task.current.submit Installed synthetic cargo check passed')
printf '%s\n' "$agent_output"
submission_id=$(printf '%s\n' "$agent_output" \
    | sed -n 's/^antdelirium\[AD-SUBMISSION\]: \(sub_[0-9a-hjkmnp-tv-z]\{26\}\)$/\1/p' \
    | tail -n 1)
[[ -n "$submission_id" ]] || fail 'the agent did not return an opaque submission receipt.'

printf '%s\n' 'AD-T8G-CYCLE: reading only the assigned submission.'
assignment_output=$(bash "$script_dir/run-reviewer-operation.sh" \
    --project-root "$target_project" -- assigned "$submission_id")
printf '%s\n' "$assignment_output"
source_context="$log_root/source-context.json"
printf '%s\n' "$assignment_output" | awk '
    /^antdelirium\[AD-SOURCE-CONTEXT\]:$/ { capture = 1; next }
    capture && /^AD-T8G-REVIEWER:/ { exit }
    capture { print }
' >"$source_context"
jq -e \
    --arg project "$project_id" \
    --arg workspace "$workspace_id" \
    --arg task "$task_id" \
    --arg submission "$submission_id" '
    .contract_version == "protected-source-context-v1" and
    .project_id == $project and
    .workspace_id == $workspace and
    .task_id == $task and
    .task_revision == 1 and
    .transition == "submission" and
    .transition_id == $submission and
    (.source_fingerprint | test("^[0-9a-f]{64}$")) and
    .changed_file_count >= 1 and
    (.changes_truncated | type == "boolean") and
    ([.changes[] | select(
        .relative_path == "src/main.rs" and .kind == "modified"
    )] | length) == 1 and
    ([.changes[].relative_path | select(
        startswith("/") or contains("\\") or
        (split("/") | any(. == ".." or . == ".git"))
    )] | length) == 0
' "$source_context" >/dev/null \
    || fail 'the assigned reviewer did not receive a valid redacted private source context.'

printf '%s\n' 'AD-T8G-CYCLE: accepting the assigned submission without implicit advancement.'
review_output=$(bash "$script_dir/run-reviewer-operation.sh" \
    --project-root "$target_project" -- \
    decide "$submission_id" accepted 'Installed synthetic evidence satisfies the task.')
printf '%s\n' "$review_output"
review_id=$(printf '%s\n' "$review_output" \
    | sed -n 's/^antdelirium\[AD-REVIEW\]: \(rev_[0-9a-hjkmnp-tv-z]\{26\}\)$/\1/p' \
    | tail -n 1)
[[ -n "$review_id" ]] || fail 'the reviewer did not return an opaque review receipt.'

jq -e --arg task "$task_id" '
    .source_revision == 1 and .task.task_id == $task and
    .queue_status == "task_available"
' "$projection" >/dev/null || fail 'review unexpectedly changed the public canonical task.'

printf '%s\n' 'AD-T8G-CYCLE: advancing only through a separately authorized CI transition.'
bash "$script_dir/run-task-advance.sh" \
    --project-root "$target_project" --review-id "$review_id"

projection_matches_completion() {
    jq -e '
        .source_revision == 2 and .task == null and
        .queue_status == "no_task_available"
    ' "$projection" >/dev/null 2>&1
}

for _attempt in {1..100}; do
    projection_matches_completion && break
    sleep 0.05
done
projection_matches_completion \
    || fail 'the supervised transition did not publish the completed canonical task within five seconds.'

printf '%s\n' 'AD-T8G-CYCLE: proving that an ordinary fully cached cargo check remains compatible.'
(
    cd "$target_project"
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        HOME="$invoking_home" \
        CARGO_HOME="$invoking_home/.cargo" \
        CARGO_TARGET_DIR="$log_root/cargo-target" \
        "$cargo_binary" check
)
projection_matches_completion \
    || fail 'the cached cargo check changed or regressed the completed projection.'
[[ ! -e "$target_project/.git" && ! -L "$target_project/.git" ]] \
    || fail 'the installed cycle created forbidden project-local Git metadata.'

{
    printf 'contract=antdelirium-installed-task-cycle-evidence-v1\n'
    printf 'external_project_access=forbidden\n'
    printf 'physical_locators=redacted\n'
    printf 'task_package_install=pass\n'
    printf 'cargo_projection_refresh=pass\n'
    printf 'agent_submission=pass\n'
    printf 'assigned_submission_read=pass\n'
    printf 'reviewer_source_context=pass\n'
    printf 'reviewer_source_change=src/main.rs:modified\n'
    printf 'assigned_review_decision=pass\n'
    printf 'scoped_ci_state_transition=pass\n'
    printf 'supervised_projection_refresh=pass\n'
    printf 'cached_cargo_check=pass\n'
    printf 'protected_history_sequence=pass\n'
    printf 'private_checkpoint_cycle=baseline,submission,review,advance\n'
    printf 'project_local_git=absent\n'
    printf 'canonical_task_advanced=yes\n'
    printf 'canonical_plan_completed=yes\n'
} >"$log_root/acceptance.env"
printf '%s\n' 'AD-T8G-CYCLE: installed synthetic task lifecycle passed.'
