#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
operator_credential=/var/lib/antdelirium-operator/state.cred
registry_database=/var/lib/antdelirium/registry/state/state.db
service_state=/var/lib/antdelirium
migration_staging=/var/lib/antdelirium/migration-staging
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$project_root/target/t86-installed-candidate-diagnostics/$run_id"
secret_root=

fail() {
    printf '%s\n' "AD-T86-CANDIDATE-DIAGNOSE: $1" >&2
    exit 1
}

finish() {
    local exit_code=$?
    set +e
    if [[ -n "$secret_root" && "$secret_root" == /run/antdelirium-candidate-diagnose.* \
        && -d "$secret_root" ]]; then
        rm -rf -- "$secret_root"
    fi
    {
        printf 'run_id=%s\nexit_code=%s\n' "$run_id" "$exit_code"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chown -R "$invoking_uid:$invoking_gid" "$log_root"
    chmod -R u+rwX,go+rX,go-w "$log_root"
    printf '%s\n' "AD-T86-CANDIDATE-DIAGNOSE: logs=$log_root"
    exit "$exit_code"
}

[[ $EUID -eq 0 ]] || fail 'execute this read-only diagnostic with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")
service_uid=$(id -u antdelirium) || fail 'the service identity is unavailable.'

for command in chmod chown date id jq mkdir mktemp python3 rm systemctl systemd-creds tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$operator_credential" && ! -L "$operator_credential" ]] \
    || fail 'the encrypted StateAdmin descriptor is unavailable.'
[[ -f "$registry_database" && ! -L "$registry_database" ]] \
    || fail 'the protected registry database is unavailable.'
[[ -d "$service_state" && ! -L "$service_state" ]] \
    || fail 'the protected service state is unavailable.'
[[ -d "$migration_staging" && ! -L "$migration_staging" ]] \
    || fail 'the protected migration staging is unavailable.'

mkdir -p "$log_root"
chmod 0755 "$(dirname -- "$log_root")" "$log_root"
exec > >(tee "$log_root/diagnostic.log") 2>&1
trap finish EXIT

secret_root=$(mktemp -d /run/antdelirium-candidate-diagnose.XXXXXX)
chmod 0700 "$secret_root"
systemd-creds --name=antdelirium-operator-state-v1 decrypt \
    "$operator_credential" "$secret_root/state.json" >/dev/null
chmod 0600 "$secret_root/state.json"

printf 'contract=antdelirium-installed-candidate-diagnostic-v1\n'
printf 'physical_locators=redacted\n'
printf 'source_mutation=forbidden\n'
printf 'service_active=%s\n' "$(systemctl is-active antdelirium.service 2>/dev/null || true)"
printf 'admin_socket_active=%s\n' "$(systemctl is-active antdelirium-admin.socket 2>/dev/null || true)"

python3 - \
    "$secret_root/state.json" \
    "$registry_database" \
    "$service_state" \
    "$migration_staging" \
    "$invoking_uid" \
    "$service_uid" <<'PY'
import base64
import hashlib
import json
import os
import sqlite3
import stat
import sys
import time

descriptor_path, database_path, state_path, staging_path, invoking_uid, service_uid = sys.argv[1:]
invoking_uid = int(invoking_uid)
service_uid = int(service_uid)
reasons = []

try:
    with open(descriptor_path, "rb") as handle:
        descriptor = json.load(handle)
    capability = descriptor.get("capability", {})
    capability_id = capability.get("capability_id")
    token = capability.get("token")
    credential_contract_ok = (
        descriptor.get("contract_version") == "admin-capability-v1"
        and set(descriptor) == {"contract_version", "capability"}
        and set(capability) == {"capability_id", "token"}
        and isinstance(capability_id, str)
        and capability_id.startswith("cap_")
        and len(capability_id) == 30
        and isinstance(token, str)
        and len(token) == 43
    )
except Exception:
    credential_contract_ok = False
    capability_id = None
    token = None

if not credential_contract_ok:
    reasons.append("credential-contract")

connection = sqlite3.connect("file:" + database_path + "?mode=ro", uri=True)
connection.execute("pragma query_only=on")
schema_version = connection.execute("pragma user_version").fetchone()[0]
quick_check = [row[0] for row in connection.execute("pragma quick_check")]

grant = None
if credential_contract_ok:
    grant = connection.execute(
        """
        select secret_hash, subject_uid, actor_kind, channel_kind, operation,
               project_id, workspace_id, task_id, not_before_unix,
               expires_at_unix, revocation_epoch
          from capability_grants
         where grant_id = ?
        """,
        (capability_id,),
    ).fetchone()

grant_found = grant is not None
secret_matches = False
subject_matches = False
authority_matches = False
scope_is_global = False
time_is_active = False
epoch_is_current = False
not_revoked = False
if grant is not None:
    padding = "=" * ((4 - len(token) % 4) % 4)
    try:
        token_bytes = base64.urlsafe_b64decode(token + padding)
        secret_matches = hashlib.sha256(token_bytes).digest() == bytes(grant[0])
    except Exception:
        secret_matches = False
    subject_matches = grant[1] == invoking_uid
    authority_matches = grant[2:5] == ("operator", "administrative", "state.admin")
    scope_is_global = grant[5:8] == (None, None, None)
    now = int(time.time())
    time_is_active = grant[8] <= now < grant[9]
    epoch = connection.execute(
        "select epoch from capability_subject_epochs where subject_uid = ? and actor_kind = ?",
        (grant[1], grant[2]),
    ).fetchone()
    epoch_is_current = epoch is not None and epoch[0] == grant[10]
    not_revoked = connection.execute(
        "select count(*) from revocations where grant_id = ?", (capability_id,)
    ).fetchone()[0] == 0

checks = {
    "credential_contract": credential_contract_ok,
    "grant_found": grant_found,
    "secret_matches": secret_matches,
    "subject_matches_invoker": subject_matches,
    "authority_is_state_admin": authority_matches,
    "scope_is_global": scope_is_global,
    "grant_time_is_active": time_is_active,
    "subject_epoch_is_current": epoch_is_current,
    "grant_not_revoked": not_revoked,
}
for name, passed in checks.items():
    print(name + "=" + ("pass" if passed else "fail"))
    if not passed:
        reasons.append(name.replace("_", "-"))

print("schema_version=" + str(schema_version))
print("quick_check=" + ("ok" if quick_check == ["ok"] else "failed"))
if schema_version != 9:
    reasons.append("schema-version")
if quick_check != ["ok"]:
    reasons.append("quick-check")

def directory_contract(path):
    metadata = os.lstat(path)
    return stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), stat.S_IMODE(metadata.st_mode), metadata.st_uid

state_is_directory, state_mode, state_uid = directory_contract(state_path)
staging_is_directory, staging_mode, staging_uid = directory_contract(staging_path)
state_ok = state_is_directory and state_mode == 0o700 and state_uid == service_uid
staging_ok = staging_is_directory and staging_mode == 0o700 and staging_uid == service_uid
print("service_state_contract=" + ("pass" if state_ok else "fail"))
print("migration_staging_contract=" + ("pass" if staging_ok else "fail"))
if not state_ok:
    reasons.append("service-state-contract")
if not staging_ok:
    reasons.append("migration-staging-contract")

partial_count = 0
complete_count = 0
unexpected_count = 0
for entry in os.scandir(staging_path):
    if not entry.is_dir(follow_symlinks=False):
        unexpected_count += 1
    elif entry.name.endswith(".partial"):
        partial_count += 1
    elif entry.name.startswith("mig_"):
        complete_count += 1
    else:
        unexpected_count += 1
print("partial_migration_count=" + str(partial_count))
print("complete_migration_count=" + str(complete_count))
print("unexpected_staging_entry_count=" + str(unexpected_count))
if unexpected_count:
    reasons.append("unexpected-staging-entry")

connection.close()
print("validation=" + ("pass" if not reasons else "fail"))
print("failure_reasons=" + (",".join(dict.fromkeys(reasons)) if reasons else "none"))
raise SystemExit(0 if not reasons else 2)
PY

printf '%s\n' 'AD-T86-CANDIDATE-DIAGNOSE: installed authorization and storage inspection completed.'
