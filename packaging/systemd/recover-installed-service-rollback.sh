#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
installed_binary=/usr/libexec/antdelirium/antdelirium
state_root=/var/lib/antdelirium
validation_parent=/var/lib/antdelirium-rollback-validations
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$repository_root/target/t8g-service-rollback-recovery-logs/$run_id"

fail() {
    printf '%s\n' "AD-T8G-SERVICE-RECOVERY: $1" >&2
    exit 1
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

start_service() {
    systemctl daemon-reload
    systemctl start antdelirium-agent.socket antdelirium-admin.socket
    systemctl start antdelirium.service
    systemctl is-active --quiet antdelirium.service
    systemctl is-active --quiet antdelirium-agent.socket
    systemctl is-active --quiet antdelirium-admin.socket
}

[[ $EUID -eq 0 ]] || fail 'execute this recovery routine with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
[[ $# -eq 2 && ${1:-} == --failed-run ]] \
    || fail 'use --failed-run <YYYYMMDDTHHMMSSZ>.'
failed_run=$2
[[ "$failed_run" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] \
    || fail 'the failed run identifier is invalid.'
validation_root="$validation_parent/$failed_run"
original_log="$repository_root/target/t8g-service-rollback-logs/$failed_run"

for command in systemctl install mv python3 sha256sum cmp chmod date tee cut; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -d "$validation_root" && ! -L "$validation_root" \
    && -f "$validation_root/current-binary" \
    && ! -L "$validation_root/current-binary" \
    && -f "$original_log/current-storage-before.env" \
    && ! -L "$original_log/current-storage-before.env" ]] \
    || fail 'the failed run does not contain a safe current-state recovery record.'

install -d -m 0755 "$log_root"
exec > >(tee -a "$log_root/recovery.log") 2>&1

printf '%s\n' 'AD-T8G-SERVICE-RECOVERY: stopping all units before selecting the newest snapshot.'
systemctl stop antdelirium.service antdelirium-agent.socket antdelirium-admin.socket
if [[ -d "$validation_root/current-state" && ! -L "$validation_root/current-state" ]]; then
    [[ ! -e "$validation_root/recovery-displaced-state" ]] \
        || fail 'the recovery destination is already occupied.'
    if [[ -d "$state_root" && ! -L "$state_root" ]]; then
        mv "$state_root" "$validation_root/recovery-displaced-state"
    else
        [[ ! -e "$state_root" ]] || fail 'the active state root is unsafe.'
    fi
    mv "$validation_root/current-state" "$state_root"
else
    [[ -d "$state_root" && ! -L "$state_root" \
        && -d "$validation_root/tested-rollback-state" ]] \
        || fail 'the newest state cannot be identified unambiguously.'
fi

install -m 0755 "$validation_root/current-binary" "$installed_binary"
[[ $(sha256sum "$installed_binary" | cut -d ' ' -f 1) \
    == $(sha256sum "$validation_root/current-binary" | cut -d ' ' -f 1) ]] \
    || fail 'the newest service binary was not restored exactly.'
validate_storage "$state_root" >"$log_root/storage-before-start.env" \
    || fail 'the recovered newest state failed integrity validation.'
cmp -s "$original_log/current-storage-before.env" "$log_root/storage-before-start.env" \
    || fail 'the recovered state does not match the pre-rollback logical snapshot.'

printf '%s\n' 'AD-T8G-SERVICE-RECOVERY: starting the restored current service.'
start_service
validate_storage "$state_root" >"$log_root/storage-after-start.env" \
    || fail 'the restored service state failed post-start integrity validation.'
cmp -s "$original_log/current-storage-before.env" "$log_root/storage-after-start.env" \
    || fail 'the restored service changed the pre-rollback logical snapshot.'

{
    printf 'contract=antdelirium-installed-service-recovery-evidence-v1\n'
    printf 'failed_run=%s\n' "$failed_run"
    printf 'current_binary_restored=exact\n'
    printf 'current_logical_snapshot=matched\n'
    printf 'storage_integrity=pass\n'
    printf 'service_active=yes\n'
    printf 'physical_locators=redacted\n'
} >"$log_root/acceptance.env"
chmod -R a+rX "$log_root"
printf '%s\n' 'AD-T8G-SERVICE-RECOVERY: current service and logical state restored.'
printf '%s\n' "AD-T8G-SERVICE-RECOVERY: logs=$log_root"
