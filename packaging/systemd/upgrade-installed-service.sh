#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bundle_root="$project_root/target/t86-system-installation-bundle"
bundle_manifest="$bundle_root/system-installation-bundle-v1.json"
staged_binary="$bundle_root/usr/libexec/antdelirium/antdelirium"
installed_binary=/usr/libexec/antdelirium/antdelirium
state_root=/var/lib/antdelirium
gate_public_pem=/var/lib/antdelirium-branch-gate/authority-public.pem
gate_public_der=/usr/lib/antdelirium/protected-gate-public.der
readiness_probe="$script_dir/probe-installed-service-ready.py"
backup_parent=/var/lib/antdelirium-upgrade-backups
run_id=$(date -u +%Y%m%dT%H%M%SZ)
backup_root="$backup_parent/$run_id"
log_root="$project_root/target/t86-capability-upgrade-logs/$run_id"
upgrade_started=false
upgrade_committed=false
artifact_divergence_recovery=false

fail() {
    printf '%s\n' "AD-T86-UPGRADE: $1" >&2
    exit 1
}

if [[ $# -eq 1 && ${1:-} == '--recover-reviewed-artifact-divergence' ]]; then
    artifact_divergence_recovery=true
elif [[ $# -ne 0 ]]; then
    fail 'usage: upgrade-installed-service.sh [--recover-reviewed-artifact-divergence]'
fi

[[ $EUID -eq 0 ]] || fail 'execute this reviewed routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
passwd_record=$(getent passwd "$invoking_user") \
    || fail 'the invoking operator has no passwd record.'
IFS=: read -r _passwd_name _passwd_secret invoking_uid invoking_gid \
    _passwd_gecos invoking_home _passwd_shell <<<"$passwd_record"
[[ "$invoking_home" == /* && -d "$invoking_home" && ! -L "$invoking_home" ]] \
    || fail 'the invoking operator home is unavailable or unsafe.'
user_wrapper="$invoking_home/.cargo/bin/antdelirium"

mkdir -p "$log_root"
chmod 0755 "$(dirname "$log_root")" "$log_root"
exec > >(tee -a "$log_root/upgrade.log") 2>&1

finish() {
    local exit_code=$?
    set +e
    if [[ $exit_code -ne 0 && $upgrade_started == true && $upgrade_committed == false ]]; then
        printf '%s\n' 'AD-T86-UPGRADE: failure detected; restoring the pre-upgrade binary and protected state.'
        systemctl stop antdelirium.service antdelirium-agent.socket antdelirium-admin.socket
        install -m 0755 "$backup_root/antdelirium" "$installed_binary"
        if [[ -d "$state_root" && ! -e "$backup_root/failed-state" ]]; then
            mv "$state_root" "$backup_root/failed-state"
        fi
        if [[ -d "$backup_root/state" ]]; then
            mv "$backup_root/state" "$state_root"
        fi
        if [[ -f "$backup_root/user-wrapper" ]]; then
            install -o "$invoking_uid" -g "$invoking_gid" -m 0755 \
                "$backup_root/user-wrapper" "$user_wrapper"
        fi
        systemctl daemon-reload
        systemctl start antdelirium-agent.socket antdelirium-admin.socket
        systemctl start antdelirium.service
    fi
    {
        printf 'run_id=%s\nexit_code=%s\nupgrade_committed=%s\n' \
            "$run_id" "$exit_code" "$upgrade_committed"
        printf 'artifact_divergence_recovery=%s\n' "$artifact_divergence_recovery"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    systemctl status antdelirium.service antdelirium-agent.socket \
        antdelirium-admin.socket --no-pager --full \
        >"$log_root/systemctl-status.txt" 2>&1
    journalctl -b -u antdelirium.service --no-pager --output=short-precise \
        >"$log_root/journal.txt" 2>&1
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T86-UPGRADE: logs=$log_root"
    exit "$exit_code"
}
trap finish EXIT

for command in systemctl journalctl jq sha256sum install cp mv python3 cmp sleep tee getent stat openssl; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$readiness_probe" && ! -L "$readiness_probe" ]] \
    || fail 'the reviewed service readiness probe is unavailable.'
[[ -f "$gate_public_pem" && ! -L "$gate_public_pem" ]] \
    || fail 'the protected gate authority must be installed before this upgrade.'
openssl pkey -pubin -in "$gate_public_pem" -outform DER \
    -out "$log_root/protected-gate-public.der" 2>"$log_root/protected-gate-public.err"
install -m 0644 -o root -g root "$log_root/protected-gate-public.der" "$gate_public_der"
[[ $(stat -c '%u:%g:%a:%s' "$gate_public_der") == '0:0:644:44' ]] \
    || fail 'the installed protected gate public key is invalid.'
[[ -f "$bundle_manifest" && -f "$staged_binary" ]] \
    || fail 'render the project-local installation bundle before running sudo.'
[[ -f "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the existing service binary is not a regular installed artifact.'
[[ -f "$user_wrapper" && ! -L "$user_wrapper" ]] \
    || fail 'the invoking operator Cargo wrapper is not a regular installed artifact.'
[[ $(stat -c '%u:%g:%a' "$user_wrapper") == "$invoking_uid:$invoking_gid:755" ]] \
    || fail 'the invoking operator Cargo wrapper ownership or mode is unsafe.'
current_service_hash=$(sha256sum "$installed_binary" | cut -d ' ' -f 1)
current_wrapper_hash=$(sha256sum "$user_wrapper" | cut -d ' ' -f 1)
if [[ "$current_service_hash" != "$current_wrapper_hash" ]]; then
    [[ "$artifact_divergence_recovery" == true ]] \
        || fail 'the installed service and Cargo wrapper do not identify the same prior release.'
    printf '%s\n' 'AD-T86-UPGRADE: explicit recovery will preserve both divergent prior artifacts before replacement.'
fi
[[ -d "$state_root" && ! -L "$state_root" ]] \
    || fail 'the protected state root is unavailable.'
jq -e '.contract_version == "system-installation-bundle-v1" and .activation_performed == false and .global_installation_performed == false' \
    "$bundle_manifest" >/dev/null || fail 'the staged bundle manifest is not reviewable.'
expected_hash=$(jq -r '.files[] | select(.artifact == "service-binary") | .sha256' "$bundle_manifest")
staged_hash=$(sha256sum "$staged_binary" | cut -d ' ' -f 1)
[[ "$expected_hash" == "$staged_hash" ]] || fail 'the staged binary digest differs from its manifest.'

for contract in service agent admin sysusers tmpfiles; do
    case "$contract" in
        service) installed=/usr/lib/systemd/system/antdelirium.service; staged="$bundle_root/usr/lib/systemd/system/antdelirium.service" ;;
        agent) installed=/usr/lib/systemd/system/antdelirium-agent.socket; staged="$bundle_root/usr/lib/systemd/system/antdelirium-agent.socket" ;;
        admin) installed=/usr/lib/systemd/system/antdelirium-admin.socket; staged="$bundle_root/usr/lib/systemd/system/antdelirium-admin.socket" ;;
        sysusers) installed=/usr/lib/sysusers.d/antdelirium.conf; staged="$bundle_root/usr/lib/sysusers.d/antdelirium.conf" ;;
        tmpfiles) installed=/usr/lib/tmpfiles.d/antdelirium.conf; staged="$bundle_root/usr/lib/tmpfiles.d/antdelirium.conf" ;;
    esac
    cmp -s "$installed" "$staged" \
        || fail 'the installed service contract differs from the reviewed bundle.'
done

printf '%s\n' 'AD-T86-UPGRADE: stopping the service and both activation sockets before replacement.'
systemctl stop antdelirium.service antdelirium-agent.socket antdelirium-admin.socket
install -d -m 0700 "$backup_parent" "$backup_root"
cp -a "$installed_binary" "$backup_root/antdelirium"
cp -a "$user_wrapper" "$backup_root/user-wrapper"
cp -a "$state_root" "$backup_root/state"
upgrade_started=true

printf '%s\n' 'AD-T86-UPGRADE: installing the reviewed binary without rebuilding as root.'
install -m 0755 "$staged_binary" "$installed_binary"
install -o "$invoking_uid" -g "$invoking_gid" -m 0755 "$staged_binary" "$user_wrapper"
[[ $(sha256sum "$installed_binary" | cut -d ' ' -f 1) == "$staged_hash" ]] \
    || fail 'the installed binary digest is not the reviewed digest.'
[[ $(sha256sum "$user_wrapper" | cut -d ' ' -f 1) == "$staged_hash" ]] \
    || fail 'the installed Cargo wrapper digest is not the reviewed digest.'

systemctl daemon-reload
systemctl start antdelirium-agent.socket antdelirium-admin.socket
systemctl start antdelirium.service
systemctl is-active --quiet antdelirium.service
systemctl is-active --quiet antdelirium-agent.socket
systemctl is-active --quiet antdelirium-admin.socket
python3 -B "$readiness_probe" /run/antdelirium-admin/admin.sock \
    || fail 'the upgraded service did not complete its administrative protocol startup.'

validate_current_storage_once() {
python3 - "$state_root" "$backup_root/state" <<'PY'
import glob
import sqlite3
import sys

root = sys.argv[1]
previous_root = sys.argv[2]
databases = [root + "/registry/state/state.db"]
databases.extend(sorted(glob.glob(root + "/projects/*/state/state.db")))
versions = []
failures = []
previous_version = -1
try:
    for ordinal, path in enumerate(databases):
        connection = sqlite3.connect("file:" + path + "?mode=ro", uri=True)
        version = connection.execute("pragma user_version").fetchone()[0]
        foreign_key_errors = connection.execute("pragma foreign_key_check").fetchall()
        quick_check = connection.execute("pragma quick_check").fetchone()[0]
        task_columns = {row[1] for row in connection.execute("pragma table_info(tasks)")}
        registration_columns = {
            row[1] for row in connection.execute(
                "pragma table_info(provisional_project_registrations)"
            )
        }
        identity_table = connection.execute(
            "select count(*) from sqlite_schema where type='table' and name='provisional_project_identities'"
        ).fetchone()[0]
        checkpoint_primary_key = {
            row[1]: row[5]
            for row in connection.execute("pragma table_info(backup_checkpoints)")
            if row[5] > 0
        }
        if version != 16:
            failures.append("database-" + str(ordinal) + ":schema-version")
        package_tables = connection.execute(
            "select count(*) from sqlite_schema where type='table' and name in "
            "('task_package_proposals', 'task_package_attestations', 'task_package_approvals', 'task_package_installations')"
        ).fetchone()[0]
        if package_tables != 4:
            failures.append("database-" + str(ordinal) + ":task-package-tables")
        if "plan_ordinal" not in {row[1] for row in connection.execute("pragma table_info(plans)")}:
            failures.append("database-" + str(ordinal) + ":plan-order")
        if "task_ordinal" not in task_columns:
            failures.append("database-" + str(ordinal) + ":task-order")
        if registration_columns != {
            "peer_uid",
            "project_identity_fingerprint",
            "workspace_identity_fingerprint",
            "project_id",
            "workspace_id",
            "project_key",
            "workspace_key",
            "created_at_unix",
        }:
            failures.append("database-" + str(ordinal) + ":identity-columns")
        if identity_table != 1:
            failures.append("database-" + str(ordinal) + ":project-identity-table")
        if checkpoint_primary_key != {
            "project_id": 1,
            "workspace_id": 2,
            "transition_kind": 3,
            "transition_id": 4,
        }:
            failures.append("database-" + str(ordinal) + ":workspace-checkpoint-key")
        if foreign_key_errors:
            failures.append("database-" + str(ordinal) + ":foreign-key-check")
        if quick_check != "ok":
            failures.append("database-" + str(ordinal) + ":quick-check")
        versions.append(version)
    registry = sqlite3.connect("file:" + databases[0] + "?mode=ro", uri=True)
    receipt_count = registry.execute("select count(*) from initial_capability_provisioning").fetchone()[0]
    grant_count = registry.execute("select count(*) from capability_grants").fetchone()[0]
    management_table = registry.execute("select count(*) from sqlite_schema where type='table' and name='capability_management_events'").fetchone()[0]
    management_sql = registry.execute("select sql from sqlite_schema where type='table' and name='capability_management_events'").fetchone()
    if receipt_count != 1:
        failures.append("registry:provisioning-receipt-count")
    if grant_count < 2:
        failures.append("registry:bootstrap-grant-count")
    if management_table != 1:
        failures.append("registry:management-table")
    if management_sql is None or "state.transition" not in management_sql[0]:
        failures.append("registry:state-transition-operation")
    if management_sql is None or any(value not in management_sql[0] for value in (
        "'operator'", "'state.admin'", "'diagnostic.private'", "'installation.admin'"
    )):
        failures.append("registry:scoped-administrative-grants")
    previous_registry = sqlite3.connect(
        "file:" + previous_root + "/registry/state/state.db?mode=ro", uri=True
    )
    previous_version = previous_registry.execute("pragma user_version").fetchone()[0]
    current_rows = registry.execute(
        "select peer_uid, project_identity_fingerprint, workspace_identity_fingerprint, project_id, workspace_id, project_key, workspace_key, created_at_unix from provisional_project_registrations order by peer_uid, workspace_identity_fingerprint"
    ).fetchall()
    if previous_version == 10:
        previous_rows = previous_registry.execute(
            "select peer_uid, identity_fingerprint, project_id, workspace_id, project_key, workspace_key, created_at_unix from provisional_project_registrations order by peer_uid, identity_fingerprint"
        ).fetchall()
        expected_rows = [
            (uid, fingerprint, fingerprint, project_id, workspace_id, project_key, workspace_key, created_at)
            for uid, fingerprint, project_id, workspace_id, project_key, workspace_key, created_at in previous_rows
        ]
    elif previous_version in (11, 12, 13, 14, 15, 16):
        expected_rows = previous_registry.execute(
            "select peer_uid, project_identity_fingerprint, workspace_identity_fingerprint, project_id, workspace_id, project_key, workspace_key, created_at_unix from provisional_project_registrations order by peer_uid, workspace_identity_fingerprint"
        ).fetchall()
    else:
        expected_rows = []
        failures.append("registry:unsupported-previous-schema")
    if current_rows != expected_rows:
        failures.append("registry:provisional-identity-preservation")
except (OSError, sqlite3.Error):
    print("validation=waiting")
    print("failure_reasons=database-busy-or-unavailable")
    raise SystemExit(2)
print("database_count=" + str(len(databases)))
print("schema_versions=" + ",".join(str(value) for value in versions))
print("previous_schema_version=" + str(previous_version))
print("provisioning_receipts=" + str(receipt_count))
print("capability_grants=" + str(grant_count))
print("capability_management_table=" + ("ready" if management_table == 1 else "unavailable"))
print("state_transition_operation=" + ("ready" if management_sql is not None and "state.transition" in management_sql[0] else "unavailable"))
print("project_workspace_identity_migration=" + ("ready" if "registry:provisional-identity-preservation" not in failures else "failed"))
print("validation=" + ("ready" if not failures else "waiting"))
print("failure_reasons=" + (",".join(failures) if failures else "none"))
raise SystemExit(0 if not failures else 2)
PY
}

storage_ready=false
for ((attempt = 1; attempt <= 100; attempt++)); do
    if validate_current_storage_once >"$log_root/storage-validation.txt" 2>&1; then
        storage_ready=true
        break
    fi
    if systemctl is-failed --quiet antdelirium.service; then
        fail 'the upgraded service failed while migrating protected storage.'
    fi
    sleep 0.1
done
[[ "$storage_ready" == true ]] \
    || fail 'timed out waiting for the protected storage-v16 migration.'

sleep 1
systemctl is-active --quiet antdelirium.service \
    || fail 'the upgraded service did not remain active after migration.'
validate_current_storage_once >"$log_root/storage-validation.txt" 2>&1 \
    || fail 'protected storage did not remain valid after service stabilization.'

upgrade_committed=true
printf '%s\n' "AD-T86-UPGRADE: service upgraded to storage-v16; rollback backup retained under $backup_root"
