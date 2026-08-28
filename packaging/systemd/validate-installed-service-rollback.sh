#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
state_root=/var/lib/antdelirium
backup_parent=/var/lib/antdelirium-upgrade-backups
validation_parent=/var/lib/antdelirium-rollback-validations
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$repository_root/target/t8g-service-rollback-logs/$run_id"
validation_root="$validation_parent/$run_id"
current_state_saved=0
current_state_restored=0
recovery=not-needed

fail() {
    printf '%s\n' "AD-T8G-SERVICE-ROLLBACK: $1" >&2
    exit 1
}

state_digest() {
    python3 - "$1" <<'PY'
import hashlib
import os
import stat
import sys

root = os.fsencode(sys.argv[1])
digest = hashlib.sha256()
file_count = 0
directory_count = 0
byte_count = 0
root_status = os.lstat(root)
if not stat.S_ISDIR(root_status.st_mode):
    raise SystemExit(2)
digest.update(
    b"r\0"
    + str(stat.S_IMODE(root_status.st_mode)).encode()
    + b"\0"
    + str(root_status.st_uid).encode()
    + b"\0"
    + str(root_status.st_gid).encode()
    + b"\0"
)
for current, directories, files in os.walk(root, topdown=True, followlinks=False):
    directories.sort()
    files.sort()
    for name in directories:
        path = os.path.join(current, name)
        status = os.lstat(path)
        if stat.S_ISLNK(status.st_mode):
            raise SystemExit(2)
        relative = os.path.relpath(path, root)
        digest.update(
            b"d\0"
            + relative
            + b"\0"
            + str(stat.S_IMODE(status.st_mode)).encode()
            + b"\0"
            + str(status.st_uid).encode()
            + b"\0"
            + str(status.st_gid).encode()
            + b"\0"
        )
        directory_count += 1
    for name in files:
        path = os.path.join(current, name)
        status = os.lstat(path)
        if not stat.S_ISREG(status.st_mode):
            raise SystemExit(2)
        relative = os.path.relpath(path, root)
        digest.update(
            b"f\0"
            + relative
            + b"\0"
            + str(stat.S_IMODE(status.st_mode)).encode()
            + b"\0"
            + str(status.st_uid).encode()
            + b"\0"
            + str(status.st_gid).encode()
            + b"\0"
        )
        with open(path, "rb", buffering=0) as handle:
            while True:
                block = handle.read(1024 * 1024)
                if not block:
                    break
                digest.update(block)
                byte_count += len(block)
        file_count += 1
print(digest.hexdigest(), file_count, directory_count, byte_count)
PY
}

validate_storage() {
    python3 - "$1" <<'PY'
import glob
import sqlite3
import sys

root = sys.argv[1]
databases = [root + "/registry/state/state.db"]
databases.extend(sorted(glob.glob(root + "/projects/*/state/state.db")))
failures = []
project_rows = 0
event_rows = 0
for ordinal, path in enumerate(databases):
    connection = sqlite3.connect("file:" + path + "?mode=ro", uri=True)
    version = connection.execute("pragma user_version").fetchone()[0]
    quick = connection.execute("pragma quick_check").fetchone()[0]
    foreign = connection.execute("pragma foreign_key_check").fetchall()
    if version != 10:
        failures.append("database-" + str(ordinal) + ":schema")
    if quick != "ok":
        failures.append("database-" + str(ordinal) + ":quick")
    if foreign:
        failures.append("database-" + str(ordinal) + ":foreign-key")
    project_rows += connection.execute("select count(*) from projects").fetchone()[0]
    event_rows += connection.execute("select count(*) from events").fetchone()[0]
print("database_count=" + str(len(databases)))
print("project_row_count=" + str(project_rows))
print("audit_event_count=" + str(event_rows))
print("schema_version=10")
print("quick_check=" + ("ok" if not any(":quick" in value for value in failures) else "failed"))
print("foreign_keys=" + ("ok" if not any(":foreign-key" in value for value in failures) else "failed"))
print("validation=" + ("pass" if not failures else "fail"))
print("failure_reasons=" + (",".join(failures) if failures else "none"))
raise SystemExit(0 if not failures else 2)
PY
}

service_contract_digest() {
    sha256sum \
        /usr/lib/systemd/system/antdelirium.service \
        /usr/lib/systemd/system/antdelirium-agent.socket \
        /usr/lib/systemd/system/antdelirium-admin.socket \
        /usr/lib/sysusers.d/antdelirium.conf \
        /usr/lib/tmpfiles.d/antdelirium.conf \
        | sha256sum | cut -d ' ' -f 1
}

start_service() {
    systemctl daemon-reload
    systemctl start antdelirium-agent.socket antdelirium-admin.socket
    systemctl start antdelirium.service
    systemctl is-active --quiet antdelirium.service
    systemctl is-active --quiet antdelirium-agent.socket
    systemctl is-active --quiet antdelirium-admin.socket
}

stop_service() {
    systemctl stop antdelirium.service antdelirium-agent.socket antdelirium-admin.socket
}

restore_current_state() {
    [[ $current_state_saved -eq 1 ]] || return 0
    if [[ $current_state_restored -eq 1 ]]; then
        start_service || return 1
        recovery=pass
        return 0
    fi
    stop_service || true
    if [[ -d "$state_root" && ! -e "$validation_root/tested-rollback-state" ]]; then
        mv "$state_root" "$validation_root/tested-rollback-state"
    elif [[ -d "$state_root" ]]; then
        mv "$state_root" "$validation_root/failed-active-state"
    fi
    [[ -d "$validation_root/current-state" ]] || return 1
    mv "$validation_root/current-state" "$state_root"
    install -m 0755 "$validation_root/current-binary" "$installed_binary"
    restored_digest=$(state_digest "$state_root") || return 1
    [[ "$restored_digest" == "$current_digest" ]] || return 1
    current_state_restored=1
    start_service || return 1
    recovery=pass
}

finish() {
    local exit_code=$?
    set +e
    if [[ $exit_code -ne 0 && $current_state_saved -eq 1 ]]; then
        if ! restore_current_state; then
            recovery=failed
            exit_code=1
        fi
    fi
    {
        printf 'contract=antdelirium-installed-service-rollback-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'recovery=%s\n' "$recovery"
        printf 'physical_locators=redacted\n'
        printf 'rollback_fixture_retained=yes\n'
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    systemctl status antdelirium.service antdelirium-agent.socket \
        antdelirium-admin.socket --no-pager --full \
        >"$log_root/systemctl-status.txt" 2>&1
    journalctl -b -u antdelirium.service --no-pager --output=short-precise \
        >"$log_root/journal.txt" 2>&1
    chmod -R a+rX "$log_root"
    printf '%s\n' "AD-T8G-SERVICE-ROLLBACK: logs=$log_root"
    exit "$exit_code"
}

[[ $EUID -eq 0 ]] || fail 'execute this reviewed rollback gate with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
[[ $# -eq 3 && ${1:-} == --backup-run && ${3:-} == --apply ]] \
    || fail 'use --backup-run <YYYYMMDDTHHMMSSZ> --apply.'
backup_run=$2
[[ "$backup_run" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] \
    || fail 'the selected backup run identifier is invalid.'
selected_backup="$backup_parent/$backup_run"

for command in systemctl journalctl install mv cp python3 sha256sum tee cmp chmod date cut; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$installed_binary" && ! -L "$installed_binary" \
    && -d "$state_root" && ! -L "$state_root" ]] \
    || fail 'the current installed service artifacts are unavailable.'
[[ -d "$selected_backup" && ! -L "$selected_backup" \
    && -f "$selected_backup/antdelirium" && ! -L "$selected_backup/antdelirium" \
    && -d "$selected_backup/state" && ! -L "$selected_backup/state" ]] \
    || fail 'the selected rollback backup is unavailable or unsafe.'
[[ ! -e "$validation_root" ]] || fail 'the protected validation destination already exists.'

install -d -m 0755 "$log_root"
install -d -m 0700 "$validation_parent" "$validation_root"
exec > >(tee -a "$log_root/supervisor.log") 2>&1
trap finish EXIT

current_binary_hash=$(sha256sum "$installed_binary" | cut -d ' ' -f 1)
rollback_binary_hash=$(sha256sum "$selected_backup/antdelirium" | cut -d ' ' -f 1)
[[ "$current_binary_hash" != "$rollback_binary_hash" ]] \
    || fail 'the selected backup does not exercise a binary rollback.'
validate_storage "$selected_backup/state" >"$log_root/selected-backup-storage.env" \
    || fail 'the selected rollback state is invalid before activation.'
rollback_state_digest=$(state_digest "$selected_backup/state") \
    || fail 'the selected rollback state contains an unsafe entry.'
contracts_before=$(service_contract_digest)

printf '%s\n' 'AD-T8G-SERVICE-ROLLBACK: stopping service before the exact current-state snapshot.'
stop_service
validate_storage "$state_root" >"$log_root/current-storage-before.env" \
    || fail 'the current protected state is invalid before rollback.'
current_digest=$(state_digest "$state_root") \
    || fail 'the current protected state contains an unsafe entry.'
cp -a "$installed_binary" "$validation_root/current-binary"
mv "$state_root" "$validation_root/current-state"
current_state_saved=1

printf '%s\n' 'AD-T8G-SERVICE-ROLLBACK: activating the selected prior binary and state snapshot.'
cp -a "$selected_backup/state" "$state_root"
[[ $(state_digest "$state_root") == "$rollback_state_digest" ]] \
    || fail 'the selected rollback state was not copied exactly.'
install -m 0755 "$selected_backup/antdelirium" "$installed_binary"
start_service
validate_storage "$state_root" >"$log_root/rollback-storage.env" \
    || fail 'the selected rollback state failed integrity validation.'
[[ $(sha256sum "$installed_binary" | cut -d ' ' -f 1) == "$rollback_binary_hash" ]] \
    || fail 'the selected rollback binary was not activated exactly.'

printf '%s\n' 'AD-T8G-SERVICE-ROLLBACK: rollback boot passed; restoring the newest state and binary.'
restore_current_state \
    || fail 'the newest service state could not be restored exactly.'
validate_storage "$state_root" >"$log_root/current-storage-restored.env" \
    || fail 'the restored newest state failed integrity validation.'
[[ $(sha256sum "$installed_binary" | cut -d ' ' -f 1) == "$current_binary_hash" ]] \
    || fail 'the newest binary was not restored exactly.'
[[ $(service_contract_digest) == "$contracts_before" ]] \
    || fail 'an installed systemd, sysusers or tmpfiles contract changed during rollback.'
cmp -s "$log_root/current-storage-before.env" "$log_root/current-storage-restored.env" \
    || fail 'logical storage counts changed across rollback and restoration.'

{
    printf 'contract=antdelirium-installed-service-rollback-evidence-v1\n'
    printf 'current_binary_sha256=%s\n' "$current_binary_hash"
    printf 'rollback_binary_sha256=%s\n' "$rollback_binary_hash"
    printf 'rollback_boot=pass\n'
    printf 'rollback_storage_integrity=pass\n'
    printf 'current_state_restored=exact\n'
    printf 'current_storage_integrity=pass\n'
    printf 'service_contracts_preserved=yes\n'
    printf 'service_active=yes\n'
    printf 'rollback_fixture_retained=yes\n'
} >"$log_root/acceptance.env"
printf '%s\n' 'AD-T8G-SERVICE-ROLLBACK: temporary rollback and exact forward restoration passed.'
