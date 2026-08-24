#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
requested_run=${1:-}
log_parent="$project_root/target/t86-capability-upgrade-diagnostics"
diagnostic_root=
log_root=
exit_code=0

fail() {
    printf '%s\n' "AD-T86-DIAGNOSE: $1" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail 'execute this read-only diagnostic with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")

if [[ -z "$requested_run" ]]; then
    requested_run=$(find "$project_root/target/t86-capability-upgrade-logs" \
        -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null \
        | sort -r | head -n 1)
fi
[[ "$requested_run" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] \
    || fail 'the upgrade run identifier is invalid.'

backup_root="/var/lib/antdelirium-upgrade-backups/$requested_run"
failed_state="$backup_root/failed-state"
[[ -d "$backup_root" && ! -L "$backup_root" ]] \
    || fail 'the rollback backup for this run is unavailable.'
[[ -d "$failed_state" && ! -L "$failed_state" ]] \
    || fail 'the preserved failed-state snapshot is unavailable.'

mkdir -p "$log_parent"
log_root="$log_parent/$requested_run"
mkdir -p "$log_root"
chmod 0755 "$log_parent" "$log_root"
exec > >(tee "$log_root/diagnostic.log") 2>&1

finish() {
    exit_code=$?
    set +e
    if [[ -n "$diagnostic_root" && -d "$diagnostic_root" ]]; then
        rm -rf -- "$diagnostic_root"
    fi
    {
        printf 'run_id=%s\nexit_code=%s\n' "$requested_run" "$exit_code"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chown -R "$invoking_uid:$invoking_gid" "$log_root"
    chmod -R u+rwX,go+rX,go-w "$log_root"
    printf '%s\n' "AD-T86-DIAGNOSE: logs=$log_root"
    exit "$exit_code"
}
trap finish EXIT

for command in chmod chown cp date dirname find head id mkdir mktemp python3 rm sort tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done

diagnostic_root=$(mktemp -d /run/antdelirium-upgrade-diagnose.XXXXXX)
chmod 0700 "$diagnostic_root"

registry_database="$failed_state/registry/state/state.db"
[[ -f "$registry_database" && ! -L "$registry_database" ]] \
    || fail 'the failed-state registry database is unavailable.'

shopt -s nullglob
project_databases=("$failed_state"/projects/*/state/state.db)
databases=("$registry_database" "${project_databases[@]}")

printf 'contract=antdelirium-upgrade-diagnostic-v1\n'
printf 'run_id=%s\n' "$requested_run"
printf 'database_count=%s\n' "${#databases[@]}"
printf 'project_database_count=%s\n' "${#project_databases[@]}"
printf 'source_mutation=forbidden\n'
printf 'physical_locators=redacted\n'

overall=pass
for index in "${!databases[@]}"; do
    database=${databases[$index]}
    state_dir=$(dirname -- "$database")
    if [[ $index -eq 0 ]]; then
        logical_scope=registry
    else
        logical_scope="project-$index"
    fi
    copy_root="$diagnostic_root/$logical_scope"
    mkdir -m 0700 "$copy_root"
    for sidecar in state.db state.db-wal state.db-shm; do
        source_file="$state_dir/$sidecar"
        if [[ ! -e "$source_file" ]]; then
            continue
        fi
        [[ -f "$source_file" && ! -L "$source_file" ]] \
            || fail 'a database sidecar is not a regular protected file.'
        cp --reflink=auto -- "$source_file" "$copy_root/$sidecar"
    done
    chmod 0600 "$copy_root"/*
    if ! python3 - "$copy_root/state.db" "$logical_scope" <<'PY'
import sqlite3
import sys

database, logical_scope = sys.argv[1:]
connection = sqlite3.connect("file:" + database + "?mode=ro", uri=True)
connection.execute("pragma query_only=on")

def table_exists(name):
    return connection.execute(
        "select count(*) from sqlite_schema where type='table' and name=?", (name,)
    ).fetchone()[0] == 1

def count_rows(name):
    return connection.execute("select count(*) from " + name).fetchone()[0]

version = connection.execute("pragma user_version").fetchone()[0]
quick_rows = [row[0] for row in connection.execute("pragma quick_check")]
quick_ok = quick_rows == ["ok"]
foreign_key_errors = list(connection.execute("pragma foreign_key_check"))
management_table = table_exists("capability_management_events")
grant_table = table_exists("capability_grants")
provisioning_table = table_exists("initial_capability_provisioning")
project_table = table_exists("projects")
grant_count = count_rows("capability_grants") if grant_table else -1
management_count = count_rows("capability_management_events") if management_table else -1
provisioning_count = count_rows("initial_capability_provisioning") if provisioning_table else -1
project_count = count_rows("projects") if project_table else -1
project_foreign_keys = 0
epoch_foreign_keys = 0
if grant_table:
    for row in connection.execute("pragma foreign_key_list(capability_grants)"):
        if row[2] == "projects":
            project_foreign_keys += 1
        if row[2] == "capability_subject_epochs":
            epoch_foreign_keys += 1

reasons = []
if version != 9:
    reasons.append("schema-version")
if not quick_ok:
    reasons.append("quick-check")
if foreign_key_errors:
    reasons.append("foreign-key-check")
if not management_table:
    reasons.append("management-table")
if project_foreign_keys != 0:
    reasons.append("premature-project-foreign-key")
if epoch_foreign_keys != 2:
    reasons.append("subject-epoch-foreign-key")
if logical_scope == "registry":
    if provisioning_count != 1:
        reasons.append("provisioning-receipt-count")
    if grant_count < 2:
        reasons.append("bootstrap-grant-count")

print("scope=" + logical_scope)
print("schema_version=" + str(version))
print("quick_check=" + ("ok" if quick_ok else "failed"))
print("foreign_key_error_count=" + str(len(foreign_key_errors)))
print("management_table=" + ("present" if management_table else "missing"))
print("management_event_count=" + str(management_count))
print("capability_grant_count=" + str(grant_count))
print("provisioning_receipt_count=" + str(provisioning_count))
print("canonical_project_count=" + str(project_count))
print("project_foreign_key_count=" + str(project_foreign_keys))
print("subject_epoch_foreign_key_count=" + str(epoch_foreign_keys))
print("validation=" + ("pass" if not reasons else "fail"))
print("failure_reasons=" + (",".join(reasons) if reasons else "none"))
print()
raise SystemExit(0 if not reasons else 2)
PY
    then
        overall=fail
    fi
done

printf 'overall=%s\n' "$overall"
[[ "$overall" == pass ]] || exit 2
printf '%s\n' 'AD-T86-DIAGNOSE: the preserved failed-state satisfies every storage-v9 invariant.'
