#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
operator_root=/var/lib/antdelirium-operator
installation_credential="$operator_root/installation.cred"
ci_credential="$operator_root/ci-attestation.cred"
admin_socket=/run/antdelirium-admin/admin.sock
run_id=$(date -u +%Y%m%dT%H%M%SZ)
secret_root="/run/antdelirium-capability-$run_id-$$"
log_root="$project_root/target/t86-capability-operator-logs/$run_id"
mode=${1:-}
admin_fd=
target_fd=
output_fd=

fail() {
    printf '%s\n' "AD-T86-CAPABILITY: $1" >&2
    exit 1
}

cleanup() {
    local exit_code=$?
    set +e
    [[ -z "$admin_fd" ]] || eval "exec ${admin_fd}<&-"
    [[ -z "$target_fd" ]] || eval "exec ${target_fd}<&-"
    [[ -z "$output_fd" ]] || eval "exec ${output_fd}>&-"
    rm -rf "$secret_root"
    {
        printf 'run_id=%s\nmode=%s\nexit_code=%s\n' "$run_id" "$mode" "$exit_code"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T86-CAPABILITY: logs=$log_root"
    exit "$exit_code"
}

[[ $EUID -eq 0 ]] || fail 'execute this supervisor routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")
[[ "$mode" == '--grant' || "$mode" == '--revoke' ]] \
    || fail 'choose exactly --grant or --revoke.'

mkdir -p "$log_root"
chmod 0755 "$(dirname "$log_root")" "$log_root"
exec > >(tee -a "$log_root/capability-operation.log") 2>&1
trap cleanup EXIT

for command in systemd-creds setpriv jq install mv id tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -x "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the installed AntDelirium binary is unavailable.'
[[ -S "$admin_socket" ]] || fail 'the administrative socket is unavailable.'
[[ -f "$installation_credential" && ! -L "$installation_credential" ]] \
    || fail 'the encrypted InstallationAdmin descriptor is unavailable.'

install -d -m 0700 "$secret_root"
admin_plain="$secret_root/installation.json"
systemd-creds --name=antdelirium-operator-installation-v1 decrypt \
    "$installation_credential" "$admin_plain" >/dev/null
chmod 0600 "$admin_plain"
jq -e '
    .contract_version == "admin-capability-v1" and
    (keys | sort) == ["capability", "contract_version"] and
    (.capability | keys | sort) == ["capability_id", "token"] and
    (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
    (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
' "$admin_plain" >/dev/null || fail 'the InstallationAdmin descriptor is invalid.'
exec {admin_fd}<"$admin_plain"
rm -f "$admin_plain"

if [[ "$mode" == '--grant' ]]; then
    project_id=${2:-}
    workspace_id=${3:-}
    lifetime_seconds=${4:-3600}
    [[ "$project_id" =~ ^prj_[0-9a-hjkmnp-tv-z]{26}$ ]] \
        || fail 'project ID is not a valid opaque ID.'
    [[ "$workspace_id" =~ ^wsp_[0-9a-hjkmnp-tv-z]{26}$ ]] \
        || fail 'workspace ID is not a valid opaque ID.'
    [[ "$lifetime_seconds" =~ ^[0-9]+$ ]] \
        && ((lifetime_seconds >= 60 && lifetime_seconds <= 86400)) \
        || fail 'lifetime must be between 60 and 86400 seconds.'
    [[ ! -e "$ci_credential" ]] \
        || fail 'an active CI attestation descriptor already exists; revoke it first.'

    output_plain="$secret_root/ci-attestation.json"
    install -m 0600 -o "$invoking_uid" -g "$invoking_gid" /dev/null "$output_plain"
    exec {output_fd}>"$output_plain"
    printf '%s\n' 'AD-T86-CAPABILITY: issuing one scoped CI attestation capability as the non-root operator.'
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
        ANTDELIRIUM_CAPABILITY_OUTPUT_FD="$output_fd" \
        ANTDELIRIUM_CAPABILITY_SUBJECT_UID="$invoking_uid" \
        ANTDELIRIUM_CAPABILITY_ACTOR=ci \
        ANTDELIRIUM_CAPABILITY_CHANNEL=administrative \
        ANTDELIRIUM_CAPABILITY_OPERATION=attestation.create \
        ANTDELIRIUM_CAPABILITY_PROJECT_ID="$project_id" \
        ANTDELIRIUM_CAPABILITY_WORKSPACE_ID="$workspace_id" \
        ANTDELIRIUM_CAPABILITY_LIFETIME_SECONDS="$lifetime_seconds" \
        ANTDELIRIUM_CAPABILITY_REVOCATION_EPOCH=0 \
        "$installed_binary" capability grant
    eval "exec ${output_fd}>&-"
    output_fd=
    jq -e '
        .contract_version == "admin-capability-v1" and
        (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
        (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
    ' "$output_plain" >/dev/null || fail 'the generated CI descriptor is invalid.'
    encrypted_partial="$secret_root/ci-attestation.cred"
    systemd-creds encrypt --with-key=host --name=antdelirium-ci-attestation-v1 \
        "$output_plain" "$encrypted_partial"
    install -m 0640 -o root -g antdelirium-admin "$encrypted_partial" \
        "$operator_root/.ci-attestation.cred.partial"
    mv "$operator_root/.ci-attestation.cred.partial" "$ci_credential"
    printf '%s\n' 'AD-T86-CAPABILITY: encrypted CI descriptor published; plaintext will be removed by the trap.'
else
    [[ $# -eq 1 ]] || fail '--revoke accepts no additional arguments.'
    [[ -f "$ci_credential" && ! -L "$ci_credential" ]] \
        || fail 'the active encrypted CI descriptor is unavailable.'
    target_plain="$secret_root/ci-attestation.json"
    systemd-creds --name=antdelirium-ci-attestation-v1 decrypt \
        "$ci_credential" "$target_plain" >/dev/null
    chmod 0600 "$target_plain"
    jq -e '
        .contract_version == "admin-capability-v1" and
        (.capability.capability_id | test("^cap_[0-9a-hjkmnp-tv-z]{26}$")) and
        (.capability.token | test("^[A-Za-z0-9_-]{43}$"))
    ' "$target_plain" >/dev/null || fail 'the CI descriptor is invalid.'
    exec {target_fd}<"$target_plain"
    rm -f "$target_plain"
    printf '%s\n' 'AD-T86-CAPABILITY: revoking the scoped CI attestation capability as the non-root operator.'
    setpriv --reuid="$invoking_uid" --regid="$invoking_gid" --init-groups \
        --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all \
        --reset-env env \
        ANTDELIRIUM_ADMIN_SOCKET="$admin_socket" \
        ANTDELIRIUM_ADMIN_CAPABILITY_FD="$admin_fd" \
        ANTDELIRIUM_CAPABILITY_TARGET_FD="$target_fd" \
        ANTDELIRIUM_CAPABILITY_REVOCATION_EPOCH=0 \
        "$installed_binary" capability revoke
    mv "$ci_credential" "$operator_root/ci-attestation.revoked-$run_id.cred"
    printf '%s\n' 'AD-T86-CAPABILITY: encrypted descriptor retained with revoked status for operator recovery.'
fi
