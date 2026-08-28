#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
operator_root=/var/lib/antdelirium-operator
installation_credential="$operator_root/installation.cred"
admin_socket=/run/antdelirium-admin/admin.sock
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
secret_root="/run/antdelirium-task-advance-$run_id"
log_root="$repository_root/target/t8g-task-advance-launcher-logs/$run_id"
admin_plain=
capability_plain=
advance_plain=
admin_fd=
output_fd=
advance_fd=
ci_fd=
target_fd=
grant_active=0
revocation_status=not-issued

fail() {
    printf '%s\n' "AD-T8G-ADVANCE: $1" >&2
    exit 1
}

close_fd() {
    local descriptor=$1
    local direction=$2
    [[ -z "$descriptor" ]] || eval "exec ${descriptor}${direction}"
}

revoke_transition_capability() {
    close_fd "$admin_fd" '<&-'
    close_fd "$target_fd" '<&-'
    admin_fd=
    target_fd=
    exec {admin_fd}<"$admin_plain"
    exec {target_fd}<"$capability_plain"
    if ! setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
        ANTDELIRIUM_CAPABILITY_TARGET_FD="$target_fd" \
        ANTDELIRIUM_CAPABILITY_REVOCATION_EPOCH=0 \
        "$installed_binary" capability revoke; then
        return 1
    fi
    grant_active=0
    revocation_status=pass
}

cleanup() {
    local exit_code=$?
    set +e
    if [[ $grant_active -eq 1 && -f "$admin_plain" && -f "$capability_plain" ]]; then
        printf '%s\n' 'AD-T8G-ADVANCE: retrying capability revocation during cleanup.'
        if ! revoke_transition_capability; then
            revocation_status=failed
        fi
    fi
    close_fd "$admin_fd" '<&-'
    close_fd "$output_fd" '>&-'
    close_fd "$advance_fd" '<&-'
    close_fd "$ci_fd" '<&-'
    close_fd "$target_fd" '<&-'
    [[ -z "$admin_plain" ]] || rm -f -- "$admin_plain"
    [[ -z "$capability_plain" ]] || rm -f -- "$capability_plain"
    [[ -z "$advance_plain" ]] || rm -f -- "$advance_plain"
    rmdir -- "$secret_root" 2>/dev/null || true
    {
        printf 'contract=antdelirium-task-advance-launcher-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'protected_plan_access=forbidden\n'
        printf 'capability_transport=inherited-descriptor\n'
        printf 'request_transport=inherited-descriptor\n'
        printf 'revocation=%s\n' "$revocation_status"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T8G-ADVANCE: logs=$log_root"
}

[[ $EUID -eq 0 ]] || fail 'execute this supervisor routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root CI subject.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")

[[ $# -eq 4 && ${1:-} == '--project-root' && -n ${2:-} && ${3:-} == '--review-id' ]] \
    || fail 'use --project-root <absolute-project> --review-id <accepted-review-id>.'
target_project=$2
review_id=$4
[[ "$target_project" == /* && -d "$target_project" && ! -L "$target_project" ]] \
    || fail 'the project root must be an absolute, existing, non-symlink directory.'
[[ "$review_id" =~ ^rev_[0-9a-hjkmnp-tv-z]{26}$ ]] \
    || fail 'the accepted review identifier is invalid.'

for command in systemd-creds setpriv jq install id tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed AntDelirium binary is unavailable.'
[[ -S "$admin_socket" ]] || fail 'the installed administrative socket is unavailable.'
[[ -f "$installation_credential" && ! -L "$installation_credential" ]] \
    || fail 'the encrypted InstallationAdmin descriptor is unavailable.'

projection="$target_project/guard/to-do_list.json"
[[ -f "$projection" && ! -L "$projection" ]] \
    || fail 'the public current-task projection is unavailable.'
jq -e '
    .contract_version == "agent-projection-v1" and
    .artifact_kind == "current_task" and
    (.project_ref | test("^prj_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.workspace_ref | test("^wsp_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.task.task_id | test("^tsk_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.source_revision | type == "number" and . >= 1)
' "$projection" >/dev/null || fail 'the public current-task projection is not actionable.'
project_id=$(jq -r '.project_ref' "$projection")
workspace_id=$(jq -r '.workspace_ref' "$projection")
task_id=$(jq -r '.task.task_id' "$projection")
state_revision=$(jq -r '.source_revision' "$projection")

umask 077
install -d -m 0755 "$log_root"
install -d -m 0700 "$secret_root"
exec > >(tee -a "$log_root/supervisor.log") 2>&1
trap cleanup EXIT

admin_plain="$secret_root/installation.json"
systemd-creds --name=antdelirium-operator-installation-v1 decrypt \
    "$installation_credential" "$admin_plain" >/dev/null
chmod 0600 "$admin_plain"
jq -e '
    .contract_version == "admin-capability-v1" and
    (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
' "$admin_plain" >/dev/null || fail 'the InstallationAdmin descriptor is invalid.'
exec {admin_fd}<"$admin_plain"

capability_plain="$secret_root/ci-state-transition.json"
install -m 0600 -o "$invoking_uid" -g "$invoking_gid" /dev/null "$capability_plain"
exec {output_fd}>"$capability_plain"
printf '%s\n' 'AD-T8G-ADVANCE: issuing one task-scoped state-transition capability.'
setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
    --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
    --reset-env env \
    ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
    ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
    ANTDELIRIUM_CAPABILITY_OUTPUT_FD="$output_fd" \
    ANTDELIRIUM_CAPABILITY_SUBJECT_UID="$invoking_uid" \
    ANTDELIRIUM_CAPABILITY_ACTOR=ci \
    ANTDELIRIUM_CAPABILITY_CHANNEL=administrative \
    ANTDELIRIUM_CAPABILITY_OPERATION=state.transition \
    ANTDELIRIUM_CAPABILITY_PROJECT_ID="$project_id" \
    ANTDELIRIUM_CAPABILITY_WORKSPACE_ID="$workspace_id" \
    ANTDELIRIUM_CAPABILITY_TASK_ID="$task_id" \
    ANTDELIRIUM_CAPABILITY_LIFETIME_SECONDS=300 \
    ANTDELIRIUM_CAPABILITY_REVOCATION_EPOCH=0 \
    "$installed_binary" capability grant
grant_active=1
revocation_status=pending
close_fd "$output_fd" '>&-'
output_fd=
jq -e '
    .contract_version == "ci-state-transition-capability-v1" and
    (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
' "$capability_plain" >/dev/null || fail 'the generated CI descriptor is invalid.'

advance_plain="$secret_root/advance.json"
jq -n \
    --arg project "$project_id" \
    --arg workspace "$workspace_id" \
    --arg task "$task_id" \
    --arg review "$review_id" \
    --argjson revision "$state_revision" \
    '{
        contract_version: "governance-task-advance-v1",
        project_id: $project,
        workspace_id: $workspace,
        task_id: $task,
        review_id: $review,
        expected_state_revision: $revision
    }' >"$advance_plain"
chmod 0600 "$advance_plain"
exec {ci_fd}<"$capability_plain"
exec {advance_fd}<"$advance_plain"
printf '%s\n' 'AD-T8G-ADVANCE: advancing the accepted canonical task through descriptor-only input.'
set +e
(
    cd "$target_project"
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$ci_fd" \
        ANTDELIRIUM_TASK_ADVANCE_FD="$advance_fd" \
        "$installed_binary" operator task advance
)
advance_exit=$?
set -e
close_fd "$ci_fd" '<&-'
close_fd "$advance_fd" '<&-'
ci_fd=
advance_fd=

printf '%s\n' 'AD-T8G-ADVANCE: revoking the one-use state-transition capability.'
if ! revoke_transition_capability; then
    fail 'the one-use state-transition capability could not be revoked.'
fi
printf '%s\n' 'AD-T8G-ADVANCE: one-use capability revoked.'
exit "$advance_exit"
