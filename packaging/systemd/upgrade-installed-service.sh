#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
bundle_root="$project_root/target/t86-system-installation-bundle"
bundle_manifest="$bundle_root/system-installation-bundle-v1.json"
staged_binary="$bundle_root/usr/libexec/antdelirium/antdelirium"
installed_binary=/usr/libexec/antdelirium/antdelirium
state_root=/var/lib/antdelirium
backup_parent=/var/lib/antdelirium-upgrade-backups
run_id=$(date -u +%Y%m%dT%H%M%SZ)
backup_root="$backup_parent/$run_id"
log_root="$project_root/target/t86-capability-upgrade-logs/$run_id"
upgrade_started=false
upgrade_committed=false

fail() {
    printf '%s\n' "AD-T86-UPGRADE: $1" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail 'execute this reviewed routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'

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
        systemctl daemon-reload
        systemctl start antdelirium-agent.socket antdelirium-admin.socket
        systemctl start antdelirium.service
    fi
    {
        printf 'run_id=%s\nexit_code=%s\nupgrade_committed=%s\n' \
            "$run_id" "$exit_code" "$upgrade_committed"
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

for command in systemctl journalctl jq sha256sum install cp mv python3 cmp sleep tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$bundle_manifest" && -f "$staged_binary" ]] \
    || fail 'render the project-local installation bundle before running sudo.'
[[ -f "$installed_binary" && ! -L "$installed_binary" ]] \
    || fail 'the existing service binary is not a regular installed artifact.'
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
cp -a "$state_root" "$backup_root/state"
upgrade_started=true

printf '%s\n' 'AD-T86-UPGRADE: installing the reviewed binary without rebuilding as root.'
install -m 0755 "$staged_binary" "$installed_binary"
[[ $(sha256sum "$installed_binary" | cut -d ' ' -f 1) == "$staged_hash" ]] \
    || fail 'the installed binary digest is not the reviewed digest.'

systemctl daemon-reload
systemctl start antdelirium-agent.socket antdelirium-admin.socket
systemctl start antdelirium.service
systemctl is-active --quiet antdelirium.service
systemctl is-active --quiet antdelirium-agent.socket
systemctl is-active --quiet antdelirium-admin.socket

validate_storage_v9_once() {
python3 - "$state_root" <<'PY'
import glob
import sqlite3
import sys

root = sys.argv[1]
databases = [root + "/registry/state/state.db"]
databases.extend(sorted(glob.glob(root + "/projects/*/state/state.db")))
versions = []
failures = []
try:
    for ordinal, path in enumerate(databases):
        connection = sqlite3.connect("file:" + path + "?mode=ro", uri=True)
        version = connection.execute("pragma user_version").fetchone()[0]
        foreign_key_errors = connection.execute("pragma foreign_key_check").fetchall()
        quick_check = connection.execute("pragma quick_check").fetchone()[0]
        if version != 9:
            failures.append("database-" + str(ordinal) + ":schema-version")
        if foreign_key_errors:
            failures.append("database-" + str(ordinal) + ":foreign-key-check")
        if quick_check != "ok":
            failures.append("database-" + str(ordinal) + ":quick-check")
        versions.append(version)
    registry = sqlite3.connect("file:" + databases[0] + "?mode=ro", uri=True)
    receipt_count = registry.execute("select count(*) from initial_capability_provisioning").fetchone()[0]
    grant_count = registry.execute("select count(*) from capability_grants").fetchone()[0]
    management_table = registry.execute("select count(*) from sqlite_schema where type='table' and name='capability_management_events'").fetchone()[0]
    if receipt_count != 1:
        failures.append("registry:provisioning-receipt-count")
    if grant_count < 2:
        failures.append("registry:bootstrap-grant-count")
    if management_table != 1:
        failures.append("registry:management-table")
except (OSError, sqlite3.Error):
    print("validation=waiting")
    print("failure_reasons=database-busy-or-unavailable")
    raise SystemExit(2)
print("database_count=" + str(len(databases)))
print("schema_versions=" + ",".join(str(value) for value in versions))
print("provisioning_receipts=" + str(receipt_count))
print("capability_grants=" + str(grant_count))
print("capability_management_table=" + ("ready" if management_table == 1 else "unavailable"))
print("validation=" + ("ready" if not failures else "waiting"))
print("failure_reasons=" + (",".join(failures) if failures else "none"))
raise SystemExit(0 if not failures else 2)
PY
}

storage_ready=false
for ((attempt = 1; attempt <= 100; attempt++)); do
    if validate_storage_v9_once >"$log_root/storage-validation.txt" 2>&1; then
        storage_ready=true
        break
    fi
    if systemctl is-failed --quiet antdelirium.service; then
        fail 'the upgraded service failed while migrating protected storage.'
    fi
    sleep 0.1
done
[[ "$storage_ready" == true ]] \
    || fail 'timed out waiting for the protected storage-v9 migration.'

sleep 1
systemctl is-active --quiet antdelirium.service \
    || fail 'the upgraded service did not remain active after migration.'
validate_storage_v9_once >"$log_root/storage-validation.txt" 2>&1 \
    || fail 'protected storage did not remain valid after service stabilization.'

upgrade_committed=true
printf '%s\n' "AD-T86-UPGRADE: service upgraded to storage-v9; rollback backup retained under $backup_root"
