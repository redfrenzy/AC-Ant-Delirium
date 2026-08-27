#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
operator_root=/var/lib/antdelirium-operator
installation_credential="$operator_root/installation.cred"
admin_socket=/run/antdelirium-admin/admin.sock
limited_socket=/run/antdelirium-agent/agent.sock
run_id=$(date -u +%Y%m%dT%H%M%SZ)-${BASHPID:-$$}
secret_root="/run/antdelirium-agent-submission-$run_id"
log_root="$repository_root/target/t8g-agent-submission-launcher-logs/$run_id"
target_project=
admin_plain=
capability_plain=
admin_fd=
output_fd=
agent_fd=
target_fd=
grant_active=0
revocation_status=not-issued

fail() {
    printf '%s\n' "AD-T8G-AGENT: $1" >&2
    exit 1
}

close_fd() {
    local descriptor=$1
    local direction=$2
    [[ -z "$descriptor" ]] || eval "exec ${descriptor}${direction}"
}

revoke_agent_capability() {
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
        printf '%s\n' 'AD-T8G-AGENT: retrying capability revocation during cleanup.'
        if ! revoke_agent_capability; then
            revocation_status=failed
        fi
    fi
    close_fd "$admin_fd" '<&-'
    close_fd "$output_fd" '>&-'
    close_fd "$agent_fd" '<&-'
    close_fd "$target_fd" '<&-'
    [[ -z "$admin_plain" ]] || rm -f -- "$admin_plain"
    [[ -z "$capability_plain" ]] || rm -f -- "$capability_plain"
    rmdir -- "$secret_root" 2>/dev/null || true
    {
        printf 'contract=antdelirium-agent-submission-launcher-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'physical_locators=redacted\n'
        printf 'protected_plan_access=forbidden\n'
        printf 'capability_transport=inherited-descriptor\n'
        printf 'revocation=%s\n' "$revocation_status"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T8G-AGENT: logs=$log_root"
}

[[ $EUID -eq 0 ]] || fail 'execute this supervisor routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root agent subject.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")

[[ ${1:-} == '--project-root' && -n ${2:-} && ${3:-} == '--' && $# -ge 5 ]] \
    || fail 'use --project-root <absolute-project> -- <existing agent arguments>.'
target_project=$2
shift 3
[[ "$target_project" == /* && -d "$target_project" && ! -L "$target_project" ]] \
    || fail 'the project root must be an absolute, existing, non-symlink directory.'
[[ ${1:-} == '--api' && ${2:-} == hierarchy.task.current.submit\ * && $# -eq 2 ]] \
    || fail 'the launcher accepts only the existing hierarchy.task.current.submit API form.'

for command in systemd-creds setpriv jq install id tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed AntDelirium binary is unavailable.'
[[ -S "$admin_socket" && -S "$limited_socket" ]] \
    || fail 'the installed administrative and limited sockets are unavailable.'
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
    (.task.task_revision | type == "number" and . >= 1)
' "$projection" >/dev/null || fail 'the public current-task projection is not actionable.'
project_id=$(jq -r '.project_ref' "$projection")
workspace_id=$(jq -r '.workspace_ref' "$projection")
task_id=$(jq -r '.task.task_id' "$projection")

umask 077
mkdir -p "$log_root"
chmod 0755 "$(dirname "$log_root")" "$log_root"
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

capability_plain="$secret_root/agent-submission.json"
install -m 0600 -o "$invoking_uid" -g "$invoking_gid" /dev/null "$capability_plain"
exec {output_fd}>"$capability_plain"
printf '%s\n' 'AD-T8G-AGENT: issuing one task-scoped submission capability.'
setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
    --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
    --reset-env env \
    ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
    ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
    ANTDELIRIUM_CAPABILITY_OUTPUT_FD="$output_fd" \
    ANTDELIRIUM_CAPABILITY_SUBJECT_UID="$invoking_uid" \
    ANTDELIRIUM_CAPABILITY_ACTOR=agent \
    ANTDELIRIUM_CAPABILITY_CHANNEL=limited \
    ANTDELIRIUM_CAPABILITY_OPERATION=submission.create \
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
    .contract_version == "agent-submission-capability-v1" and
    (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
' "$capability_plain" >/dev/null || fail 'the generated agent descriptor is invalid.'

exec {agent_fd}<"$capability_plain"
printf '%s\n' 'AD-T8G-AGENT: invoking the existing agent API with descriptor-only authority.'
set +e
(
    cd "$target_project"
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_LIMITED_SOCKET="$limited_socket" \
        ANTDELIRIUM_AGENT_CAPABILITY_FD="$agent_fd" \
        "$installed_binary" "$@"
)
agent_exit=$?
set -e
close_fd "$agent_fd" '<&-'
agent_fd=

printf '%s\n' 'AD-T8G-AGENT: revoking the one-use submission capability.'
if ! revoke_agent_capability; then
    fail 'the one-use submission capability could not be revoked.'
fi
printf '%s\n' 'AD-T8G-AGENT: one-use capability revoked.'
exit "$agent_exit"
