#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
state_root=/var/lib/antdelirium
installed_binary=/usr/libexec/antdelirium/antdelirium
bundle_binary="$repository_root/target/t86-system-installation-bundle/usr/libexec/antdelirium/antdelirium"
release_binary="$repository_root/target/x86_64-unknown-linux-gnu/release/antdelirium"
integrity_authority=/usr/libexec/antdelirium/checkpoint-authority.py
integrity_root=/var/lib/antdelirium-integrity
integrity_public_key="$integrity_root/authority-public.pem"
run_id=$(date -u +%Y%m%dT%H%M%SZ)
log_root="$repository_root/target/t110-installed-regression-diagnostics/$run_id"

fail() {
    printf '%s\n' "AD-T110-DIAGNOSE: $1" >&2
    exit 1
}

[[ $EUID -eq 0 ]] || fail 'execute this read-only diagnostic with sudo.'
invoking_user=${SUDO_USER:-}
[[ -n "$invoking_user" && "$invoking_user" != root ]] \
    || fail 'SUDO_USER must identify the non-root operator.'
invoking_uid=$(id -u "$invoking_user")
invoking_gid=$(id -g "$invoking_user")
operator_wrapper="/home/$invoking_user/.cargo/bin/antdelirium"

for command in chmod chown cut date id mkdir python3 sha256sum systemctl tee; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
for artifact in "$installed_binary" "$operator_wrapper" "$bundle_binary" "$release_binary"; do
    [[ -f "$artifact" && ! -L "$artifact" ]] \
        || fail 'one required binary artifact is unavailable or unsafe.'
done
[[ -d "$state_root" && ! -L "$state_root" ]] \
    || fail 'the protected state root is unavailable or unsafe.'

mkdir -p "$log_root"
chmod 0755 "$(dirname -- "$log_root")" "$log_root"
exec > >(tee "$log_root/diagnostic.log") 2>&1

finish() {
    local exit_code=$?
    set +e
    {
        printf 'contract=antdelirium-installed-regression-diagnostic-v1\n'
        printf 'run_id=%s\n' "$run_id"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$log_root/result.env"
    chown -R "$invoking_uid:$invoking_gid" "$log_root"
    chmod -R u+rwX,go+rX,go-w "$log_root"
    printf '%s\n' "AD-T110-DIAGNOSE: logs=$log_root"
    exit "$exit_code"
}
trap finish EXIT

installed_sha256=$(sha256sum "$installed_binary" | cut -d ' ' -f 1)
wrapper_sha256=$(sha256sum "$operator_wrapper" | cut -d ' ' -f 1)
bundle_sha256=$(sha256sum "$bundle_binary" | cut -d ' ' -f 1)
release_sha256=$(sha256sum "$release_binary" | cut -d ' ' -f 1)

printf 'contract=antdelirium-installed-regression-diagnostic-v1\n'
printf 'physical_locators=redacted\n'
printf 'source_mutation=forbidden\n'
printf 'installed_sha256=%s\n' "$installed_sha256"
printf 'wrapper_sha256=%s\n' "$wrapper_sha256"
printf 'bundle_sha256=%s\n' "$bundle_sha256"
printf 'release_sha256=%s\n' "$release_sha256"
printf 'reviewed_artifact_parity=%s\n' \
    "$([[ "$bundle_sha256" == "$release_sha256" ]] && printf pass || printf fail)"
printf 'installed_artifact_parity=%s\n' \
    "$([[ "$installed_sha256" == "$release_sha256" && "$wrapper_sha256" == "$release_sha256" ]] && printf pass || printf fail)"
if systemctl is-active --quiet antdelirium.service; then
    printf 'service_active=active\n'
else
    printf 'service_active=inactive\n'
fi

checkpoint_validation=unavailable
if [[ -f "$integrity_authority" && ! -L "$integrity_authority" \
    && -f "$integrity_public_key" && ! -L "$integrity_public_key" \
    && -f "$integrity_root/checkpoint-envelope.json" \
    && ! -L "$integrity_root/checkpoint-envelope.json" ]]; then
    if python3 -B "$integrity_authority" verify \
        --state-root "$state_root" \
        --anchor-root "$integrity_root" \
        --public-key "$integrity_public_key" \
        >"$log_root/checkpoint.out" 2>"$log_root/checkpoint.err"; then
        checkpoint_validation=pass
    else
        checkpoint_validation=changed
    fi
fi
printf 'signed_checkpoint_validation=%s\n' "$checkpoint_validation"

python3 -B - "$state_root" <<'PY'
import sqlite3
import sys
import time
from pathlib import Path

state_root = Path(sys.argv[1])
registry_path = state_root / "registry" / "state" / "state.db"
project_paths = sorted((state_root / "projects").glob("*/state/state.db"))
database_paths = [registry_path, *project_paths]

def connect(path):
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.execute("pragma query_only=on")
    return connection

def table_exists(connection, name):
    return connection.execute(
        "select count(*) from sqlite_schema where type='table' and name=?", (name,)
    ).fetchone()[0] == 1

quick_check_failures = 0
foreign_key_error_count = 0
schema_versions = []
synthetic_baseline_task_count = 0
invalid_canonical_pointer_count = 0

for path in database_paths:
    connection = connect(path)
    schema_versions.append(connection.execute("pragma user_version").fetchone()[0])
    if connection.execute("pragma quick_check").fetchone()[0] != "ok":
        quick_check_failures += 1
    foreign_key_error_count += len(connection.execute("pragma foreign_key_check").fetchall())
    if table_exists(connection, "tasks"):
        synthetic_baseline_task_count += connection.execute(
            "select count(*) from tasks where lower(cast(payload as text)) like '%governance baseline%'"
        ).fetchone()[0]
        invalid_canonical_pointer_count += connection.execute(
            """
            select count(*)
            from projects p
            left join plans ap on ap.plan_id = p.active_plan_id and ap.project_id = p.project_id
            left join tasks ct on ct.task_id = p.current_task_id and ct.project_id = p.project_id
            where (p.active_plan_id is not null and ap.plan_id is null)
               or (p.current_task_id is not null and ct.task_id is null)
               or (ct.task_id is not null and p.active_plan_id != ct.plan_id)
            """
        ).fetchone()[0]
    connection.close()

registry = connect(registry_path)
candidate_project_count = registry.execute(
    "select count(*) from projects where project_key like 'legacy-candidate--%'"
).fetchone()[0]
candidate_association_count = registry.execute(
    """
    select count(*)
    from provisional_project_registrations r
    join projects p on p.project_id = r.project_id
    where p.project_key like 'legacy-candidate--%'
    """
).fetchone()[0]
identity_contradiction_count = registry.execute(
    """
    select count(*)
    from provisional_project_registrations r
    left join provisional_project_identities i
      on i.peer_uid = r.peer_uid
     and i.project_identity_fingerprint = r.project_identity_fingerprint
     and i.project_id = r.project_id
    where i.project_id is null
    """
).fetchone()[0]
scope_contradiction_count = registry.execute(
    """
    select count(*)
    from provisional_project_registrations r
    left join projects p on p.project_id = r.project_id and p.project_key = r.project_key
    left join workspaces w
      on w.workspace_id = r.workspace_id
     and w.project_id = r.project_id
     and w.workspace_key = r.workspace_key
    where p.project_id is null or w.workspace_id is null
    """
).fetchone()[0]
active_capability_count = registry.execute(
    """
    select count(*)
    from capability_grants g
    left join revocations r on r.grant_id = g.grant_id
    left join capability_subject_epochs e
      on e.subject_uid = g.subject_uid and e.actor_kind = g.actor_kind
    where r.grant_id is null
      and g.not_before_unix <= ?
      and g.expires_at_unix > ?
      and e.epoch = g.revocation_epoch
    """,
    (int(time.time()), int(time.time())),
).fetchone()[0]
registry.close()

state_recovery_required = any(
    (
        quick_check_failures,
        foreign_key_error_count,
        candidate_association_count,
        identity_contradiction_count,
        scope_contradiction_count,
        synthetic_baseline_task_count,
        invalid_canonical_pointer_count,
    )
)

print(f"database_count={len(database_paths)}")
print("schema_versions=" + ",".join(str(version) for version in schema_versions))
print(f"quick_check_failure_count={quick_check_failures}")
print(f"foreign_key_error_count={foreign_key_error_count}")
print(f"candidate_project_count={candidate_project_count}")
print(f"candidate_association_count={candidate_association_count}")
print(f"identity_contradiction_count={identity_contradiction_count}")
print(f"scope_contradiction_count={scope_contradiction_count}")
print(f"synthetic_baseline_task_count={synthetic_baseline_task_count}")
print(f"invalid_canonical_pointer_count={invalid_canonical_pointer_count}")
print(f"active_capability_count={active_capability_count}")
print(f"state_recovery_required={'yes' if state_recovery_required else 'no'}")
PY

printf 'validation=complete\n'
printf '%s\n' 'AD-T110-DIAGNOSE: installed binary and protected state inspection completed.'
