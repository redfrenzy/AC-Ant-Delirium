#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
authority="$script_dir/checkpoint-authority.py"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
log_root="$repository_root/target/t96-signed-checkpoint-logs/$run_id"
state_root="$log_root/protected-state"
anchor_root="$log_root/external-authority"
private_key="$anchor_root/authority-private.pem"
public_key="$anchor_root/authority-public.pem"
project_database="$state_root/projects/fixture-project/state/state.db"
envelope="$anchor_root/checkpoint-envelope.json"

fail() {
    printf '%s\n' "AD-T96-INTEGRITY: $1" >&2
    exit 1
}

expect_rejection() {
    local expected=$1
    shift
    if "$@" >"$log_root/rejection.out" 2>"$log_root/rejection.err"; then
        fail 'a hostile integrity scenario was accepted.'
    fi
    grep -Fq "$expected" "$log_root/rejection.err" \
        || fail 'a hostile integrity scenario returned the wrong rejection.'
    if rg -n '/home/|/var/lib/|state\.db|fixture-project' \
        "$log_root/rejection.out" "$log_root/rejection.err" >/dev/null; then
        fail 'an integrity rejection exposed a physical locator.'
    fi
}

for command in python3 openssl cp chmod grep rg; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$authority" && ! -L "$authority" ]] \
    || fail 'the reviewed integrity authority is unavailable.'

mkdir -p "$state_root/registry/state" \
    "$state_root/projects/fixture-project/state" "$anchor_root"
chmod 0700 "$state_root" "$state_root/registry" "$state_root/registry/state" \
    "$state_root/projects" "$state_root/projects/fixture-project" \
    "$state_root/projects/fixture-project/state" "$anchor_root"

python3 - "$state_root" <<'PY'
import pathlib
import sqlite3
import sys

root = pathlib.Path(sys.argv[1])

def snapshot_fingerprint(project_id, workspace_id, revision, payload):
    import hashlib
    material = bytearray()
    for value in ("canonical-governance-snapshot-v1", project_id, workspace_id):
        encoded = value.encode("utf-8")
        material.extend(len(encoded).to_bytes(4, "little"))
        material.extend(encoded)
    material.extend(revision.to_bytes(8, "little", signed=True))
    material.extend(len(payload).to_bytes(4, "little"))
    material.extend(payload)
    return hashlib.sha256(material).hexdigest()

schema = """
CREATE TABLE projects(
    project_id TEXT PRIMARY KEY,
    project_key TEXT NOT NULL UNIQUE,
    state_revision INTEGER NOT NULL,
    last_event_sequence INTEGER NOT NULL
);
CREATE TABLE events(
    project_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    event_id TEXT NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY(project_id, sequence)
);
CREATE TABLE service_security_state(
    singleton INTEGER PRIMARY KEY,
    last_sequence INTEGER NOT NULL
);
CREATE TABLE service_security_events(
    sequence INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE capability_management_events(
    sequence INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL,
    payload BLOB NOT NULL
);
CREATE TABLE governance_snapshots(
    project_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY(project_id, workspace_id)
);
PRAGMA user_version = 13;
"""

registry = sqlite3.connect(root / "registry/state/state.db")
registry.executescript(schema)
registry.execute(
    "INSERT INTO projects VALUES (?, ?, ?, ?)",
    ("prj_fixture", "fixture-project", 0, 0),
)
registry.execute("INSERT INTO service_security_state VALUES (1, 2)")
registry.execute(
    "INSERT INTO service_security_events VALUES (?, ?, ?)",
    (1, "evt_denied", b"denied request"),
)
registry.execute(
    "INSERT INTO capability_management_events VALUES (?, ?, ?)",
    (2, "evt_granted", b"managed grant"),
)
registry.commit()
registry.close()

payload = b'{"task":"initial"}'
project = sqlite3.connect(root / "projects/fixture-project/state/state.db")
project.executescript(schema)
project.execute(
    "INSERT INTO projects VALUES (?, ?, ?, ?)",
    ("prj_fixture", "fixture-project", 1, 1),
)
project.execute(
    "INSERT INTO events VALUES (?, ?, ?, ?)",
    ("prj_fixture", 1, "evt_initial", b"initial event"),
)
project.execute("INSERT INTO service_security_state VALUES (1, 0)")
project.execute(
    "INSERT INTO governance_snapshots VALUES (?, ?, ?, ?, ?)",
    (
        "prj_fixture",
        "wsp_fixture",
        1,
        snapshot_fingerprint("prj_fixture", "wsp_fixture", 1, payload),
        payload,
    ),
)
project.commit()
project.close()
PY
chmod 0600 "$state_root/registry/state/state.db" "$project_database"

openssl genpkey -algorithm ED25519 -out "$private_key" 2>"$log_root/keygen.err"
chmod 0600 "$private_key"
openssl pkey -in "$private_key" -pubout -out "$public_key" 2>"$log_root/public-key.err"
chmod 0644 "$public_key"

printf '%s\n' 'AD-T96-INTEGRITY: sealing the first externally anchored checkpoint.'
python3 -B "$authority" seal \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --private-key "$private_key" \
    --public-key "$public_key" >"$log_root/epoch-1.out"
cp "$project_database" "$log_root/project-epoch-1.db"

printf '%s\n' 'AD-T96-INTEGRITY: extending a valid storage-v10 anchor across the reviewed schema upgrade.'
upgrade_anchor="$log_root/schema-upgrade-authority"
mkdir -m 0700 "$upgrade_anchor"
cp "$private_key" "$upgrade_anchor/authority-private.pem"
cp "$public_key" "$upgrade_anchor/authority-public.pem"
chmod 0600 "$upgrade_anchor/authority-private.pem"
chmod 0644 "$upgrade_anchor/authority-public.pem"
python3 - "$envelope" "$upgrade_anchor/checkpoint.json" <<'PY'
import json
import pathlib
import sys

envelope = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
checkpoint = envelope["checkpoint"]
checkpoint["storage_schema_version"] = 10
for database in checkpoint["databases"]:
    database["schema_version"] = 10
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(checkpoint, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="ascii",
)
PY
openssl pkeyutl -sign \
    -inkey "$upgrade_anchor/authority-private.pem" \
    -rawin \
    -in "$upgrade_anchor/checkpoint.json" \
    -out "$upgrade_anchor/checkpoint.sig"
python3 - "$upgrade_anchor/checkpoint.json" "$upgrade_anchor/checkpoint.sig" \
    "$upgrade_anchor/checkpoint-envelope.json" <<'PY'
import base64
import json
import pathlib
import sys

checkpoint = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
signature = base64.b64encode(pathlib.Path(sys.argv[2]).read_bytes()).decode("ascii")
envelope = {
    "contract_version": "signed-integrity-checkpoint-v1",
    "checkpoint": checkpoint,
    "signature_base64": signature,
}
pathlib.Path(sys.argv[3]).write_text(
    json.dumps(envelope, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="ascii",
)
PY
chmod 0600 "$upgrade_anchor/checkpoint-envelope.json"
python3 -B "$authority" seal \
    --state-root "$state_root" \
    --anchor-root "$upgrade_anchor" \
    --private-key "$upgrade_anchor/authority-private.pem" \
    --public-key "$upgrade_anchor/authority-public.pem" \
    >"$log_root/schema-upgrade.out"
python3 -B "$authority" verify \
    --state-root "$state_root" \
    --anchor-root "$upgrade_anchor" \
    --public-key "$upgrade_anchor/authority-public.pem" \
    >"$log_root/schema-upgrade-verify.out"
jq -e '
    .checkpoint.epoch == 2 and
    .checkpoint.storage_schema_version == 13 and
    (.checkpoint.previous_checkpoint_sha256 | test("^[0-9a-f]{64}$")) and
    all(.checkpoint.databases[]; .schema_version == 13)
' "$upgrade_anchor/checkpoint-envelope.json" >/dev/null \
    || fail 'the signed storage schema transition was not anchored correctly.'

python3 - "$project_database" <<'PY'
import sqlite3
import sys

path = sys.argv[1]
payload = b'{"task":"advanced"}'
def snapshot_fingerprint(project_id, workspace_id, revision, payload):
    import hashlib
    material = bytearray()
    for value in ("canonical-governance-snapshot-v1", project_id, workspace_id):
        encoded = value.encode("utf-8")
        material.extend(len(encoded).to_bytes(4, "little"))
        material.extend(encoded)
    material.extend(revision.to_bytes(8, "little", signed=True))
    material.extend(len(payload).to_bytes(4, "little"))
    material.extend(payload)
    return hashlib.sha256(material).hexdigest()
connection = sqlite3.connect(path)
connection.execute(
    "INSERT INTO events VALUES (?, ?, ?, ?)",
    ("prj_fixture", 2, "evt_advanced", b"advanced event"),
)
connection.execute(
    "UPDATE projects SET state_revision = 2, last_event_sequence = 2 "
    "WHERE project_id = 'prj_fixture'"
)
connection.execute(
    "UPDATE governance_snapshots SET source_revision = 2, fingerprint = ?, payload = ?",
    (snapshot_fingerprint("prj_fixture", "wsp_fixture", 2, payload), payload),
)
connection.commit()
connection.close()
PY

printf '%s\n' 'AD-T96-INTEGRITY: extending the signed journal and governance snapshot.'
python3 -B "$authority" seal \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --private-key "$private_key" \
    --public-key "$public_key" >"$log_root/epoch-2.out"
python3 -B "$authority" verify \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --public-key "$public_key" >"$log_root/verify-epoch-2.out"
cp "$project_database" "$log_root/project-epoch-2.db"
cp "$envelope" "$log_root/envelope-epoch-2.json"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting a sequence gap.'
python3 - "$project_database" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(sys.argv[1])
connection.execute("DELETE FROM events WHERE sequence = 1")
connection.commit()
connection.close()
PY
expect_rejection 'journal sequence gap detected' \
    python3 -B "$authority" seal \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --private-key "$private_key" --public-key "$public_key"
cp "$log_root/project-epoch-2.db" "$project_database"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting a gap in the interleaved service authority journal.'
cp "$state_root/registry/state/state.db" "$log_root/registry-epoch-2.db"
python3 - "$state_root/registry/state/state.db" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(sys.argv[1])
connection.execute("DELETE FROM service_security_events WHERE sequence = 1")
connection.commit()
connection.close()
PY
expect_rejection 'journal sequence gap detected: service-authority' \
    python3 -B "$authority" seal \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --private-key "$private_key" --public-key "$public_key"
cp "$log_root/registry-epoch-2.db" "$state_root/registry/state/state.db"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting a rewritten signed prefix.'
python3 - "$project_database" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(sys.argv[1])
connection.execute("UPDATE events SET payload = ? WHERE sequence = 1", (b"rewritten",))
connection.commit()
connection.close()
PY
expect_rejection 'signed journal history changed' \
    python3 -B "$authority" seal \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --private-key "$private_key" --public-key "$public_key"
cp "$log_root/project-epoch-2.db" "$project_database"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting rollback behind the signed epoch.'
cp "$log_root/project-epoch-1.db" "$project_database"
expect_rejection 'signed journal history changed' \
    python3 -B "$authority" seal \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --private-key "$private_key" --public-key "$public_key"
cp "$log_root/project-epoch-2.db" "$project_database"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting an unsigned snapshot mutation.'
python3 - "$project_database" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(sys.argv[1])
connection.execute("UPDATE governance_snapshots SET payload = ?", (b"tampered",))
connection.commit()
connection.close()
PY
expect_rejection 'governance snapshot fingerprint mismatch' \
    python3 -B "$authority" verify \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --public-key "$public_key"
cp "$log_root/project-epoch-2.db" "$project_database"

printf '%s\n' 'AD-T96-INTEGRITY: rejecting a modified Ed25519 envelope.'
python3 - "$envelope" <<'PY'
import json
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
envelope = json.loads(path.read_text(encoding="ascii"))
signature = envelope["signature_base64"]
envelope["signature_base64"] = ("A" if signature[0] != "A" else "B") + signature[1:]
path.write_text(
    json.dumps(envelope, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="ascii",
)
PY
expect_rejection 'checkpoint signature verification failed' \
    python3 -B "$authority" verify \
    --state-root "$state_root" --anchor-root "$anchor_root" \
    --public-key "$public_key"
cp "$log_root/envelope-epoch-2.json" "$envelope"

python3 -B "$authority" verify \
    --state-root "$state_root" \
    --anchor-root "$anchor_root" \
    --public-key "$public_key" >"$log_root/final-verify.out"

python3 - "$envelope" "$log_root/evidence.json" <<'PY'
import json
import pathlib
import sys
envelope = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
checkpoint = envelope["checkpoint"]
evidence = {
    "contract_version": "signed-integrity-checkpoint-evidence-v1",
    "physical_locators": "redacted",
    "external_authority": True,
    "private_key_outside_service_state": True,
    "checkpoint_epoch": checkpoint["epoch"],
    "database_count": checkpoint["database_count"],
    "service_authority_interleaving_validated": True,
    "journal_gap_rejected": True,
    "journal_prefix_rewrite_rejected": True,
    "state_rollback_rejected": True,
    "snapshot_mutation_rejected": True,
    "signature_mutation_rejected": True,
    "validation": "pass",
}
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="ascii"
)
PY

printf '%s\n' 'AD-T96-INTEGRITY: signed checkpoint, journal continuity and rollback matrix passed.'
printf '%s\n' "AD-T96-INTEGRITY: logs=$log_root"
