#!/usr/bin/env python3
"""Seal and verify protected AntDelirium state under an external authority.

The service state is deliberately only an input.  The signed checkpoint and the
Ed25519 private key live in an operator-owned root that the service cannot
modify, so restoring the SQLite state cannot also restore the monotonic anchor.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import stat
import subprocess
import sys
import tempfile
from typing import Any, Iterable


CONTRACT_VERSION = "integrity-checkpoint-v1"
ENVELOPE_CONTRACT_VERSION = "signed-integrity-checkpoint-v1"
SCHEMA_VERSION = 13
SUPPORTED_SIGNED_SCHEMA_VERSIONS = frozenset((10, 11, 12, 13))
CHAIN_SEED = hashlib.sha256(b"antdelirium-event-chain-v1\0").digest()
SNAPSHOT_CONTRACT_VERSION = "canonical-governance-snapshot-v1"


class IntegrityFailure(Exception):
    """Expected fail-closed integrity rejection."""


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=True,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("ascii") + b"\n"


def require_directory(path: Path) -> None:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise IntegrityFailure("required directory is unavailable") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        raise IntegrityFailure("unsafe directory metadata")


def require_regular_file(path: Path) -> None:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise IntegrityFailure("required file is unavailable") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise IntegrityFailure("unsafe file metadata")


def open_database(path: Path) -> sqlite3.Connection:
    require_regular_file(path)
    try:
        connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        connection.execute("PRAGMA query_only = ON")
        if connection.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise IntegrityFailure("database quick check failed")
        if connection.execute("PRAGMA foreign_key_check").fetchone() is not None:
            raise IntegrityFailure("database foreign key check failed")
        if connection.execute("PRAGMA user_version").fetchone()[0] != SCHEMA_VERSION:
            raise IntegrityFailure("unsupported storage schema")
        return connection
    except sqlite3.Error as error:
        raise IntegrityFailure("database inspection failed") from error


def encode_value(value: Any) -> bytes:
    if value is None:
        payload = b""
        tag = b"n"
    elif isinstance(value, bytes):
        payload = value
        tag = b"b"
    elif isinstance(value, int):
        payload = str(value).encode("ascii")
        tag = b"i"
    elif isinstance(value, float):
        payload = value.hex().encode("ascii")
        tag = b"f"
    elif isinstance(value, str):
        payload = value.encode("utf-8")
        tag = b"s"
    else:
        raise IntegrityFailure("unsupported database value")
    return tag + len(payload).to_bytes(8, "big") + payload


def encode_row(row: Iterable[Any]) -> bytes:
    encoded = bytearray()
    for value in row:
        encoded.extend(encode_value(value))
    return bytes(encoded)


def snapshot_fingerprint(
    project_id: str, workspace_id: str, source_revision: int, payload: bytes
) -> str:
    material = bytearray()
    for value in (SNAPSHOT_CONTRACT_VERSION, project_id, workspace_id):
        encoded = value.encode("utf-8")
        if len(encoded) > 0xFFFFFFFF:
            raise IntegrityFailure("governance snapshot identity is too large")
        material.extend(len(encoded).to_bytes(4, "little"))
        material.extend(encoded)
    if source_revision < 0 or source_revision > 0x7FFFFFFFFFFFFFFF:
        raise IntegrityFailure("governance snapshot revision is invalid")
    material.extend(source_revision.to_bytes(8, "little", signed=True))
    if len(payload) > 0xFFFFFFFF:
        raise IntegrityFailure("governance snapshot payload is too large")
    material.extend(len(payload).to_bytes(4, "little"))
    material.extend(payload)
    return sha256_hex(bytes(material))


def quoted(identifier: str) -> str:
    return '"' + identifier.replace('"', '""') + '"'


def semantic_database_sha256(connection: sqlite3.Connection) -> str:
    digest = hashlib.sha256(b"antdelirium-semantic-database-v1\0")
    tables = [
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_schema "
            "WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
    ]
    for table in tables:
        columns = [row[1] for row in connection.execute(f"PRAGMA table_info({quoted(table)})")]
        if not columns:
            raise IntegrityFailure("table without columns")
        select_columns = ",".join(quoted(column) for column in columns)
        order_columns = ",".join(quoted(column) for column in columns)
        digest.update(encode_value(table))
        digest.update(encode_row(columns))
        try:
            rows = connection.execute(
                f"SELECT {select_columns} FROM {quoted(table)} ORDER BY {order_columns}"
            )
            for row in rows:
                digest.update(encode_row(row))
        except sqlite3.Error as error:
            raise IntegrityFailure("semantic database inspection failed") from error
    return digest.hexdigest()


def table_columns(connection: sqlite3.Connection, table: str) -> list[str]:
    columns = [row[1] for row in connection.execute(f"PRAGMA table_info({quoted(table)})")]
    if not columns:
        raise IntegrityFailure("required journal table is unavailable")
    return columns


def event_stream(
    connection: sqlite3.Connection,
    table: str,
    kind: str,
    subject_id: str | None,
    expected_last: int,
    where_sql: str = "",
    parameters: tuple[Any, ...] = (),
    previous: dict[str, Any] | None = None,
) -> dict[str, Any]:
    columns = table_columns(connection, table)
    select_columns = ",".join(quoted(column) for column in columns)
    clause = f" WHERE {where_sql}" if where_sql else ""
    rows = list(
        connection.execute(
            f"SELECT {select_columns} FROM {quoted(table)}{clause} ORDER BY sequence",
            parameters,
        )
    )
    sequences = [int(row[columns.index("sequence")]) for row in rows]
    if expected_last < 0:
        raise IntegrityFailure("negative journal sequence")
    if sequences != list(range(1, expected_last + 1)):
        raise IntegrityFailure(f"journal sequence gap detected: {kind}")
    head = CHAIN_SEED
    previous_head = None
    for sequence, row in zip(sequences, rows, strict=True):
        head = hashlib.sha256(head + encode_row(row)).digest()
        if previous is not None and sequence == int(previous["last_sequence"]):
            previous_head = head.hex()
    if previous is not None:
        previous_last = int(previous["last_sequence"])
        if previous_last == 0:
            previous_head = CHAIN_SEED.hex()
        if previous_last > expected_last or previous_head != previous["chain_sha256"]:
            raise IntegrityFailure("signed journal history changed")
    return {
        "kind": kind,
        "subject_id": subject_id,
        "last_sequence": expected_last,
        "chain_sha256": head.hex(),
    }


def service_authority_stream(
    connection: sqlite3.Connection,
    expected_last: int,
    previous: dict[str, Any] | None,
) -> dict[str, Any]:
    combined_rows = []
    for table, row_kind in (
        ("service_security_events", "service-security"),
        ("capability_management_events", "capability-management"),
    ):
        columns = table_columns(connection, table)
        select_columns = ",".join(quoted(column) for column in columns)
        sequence_index = columns.index("sequence")
        for row in connection.execute(
            f"SELECT {select_columns} FROM {quoted(table)} ORDER BY sequence"
        ):
            combined_rows.append((int(row[sequence_index]), row_kind, row))
    combined_rows.sort(key=lambda item: item[0])
    sequences = [item[0] for item in combined_rows]
    if expected_last < 0:
        raise IntegrityFailure("negative journal sequence")
    if sequences != list(range(1, expected_last + 1)):
        raise IntegrityFailure("journal sequence gap detected: service-authority")

    head = CHAIN_SEED
    previous_head = None
    for sequence, row_kind, row in combined_rows:
        head = hashlib.sha256(head + encode_value(row_kind) + encode_row(row)).digest()
        if previous is not None and sequence == int(previous["last_sequence"]):
            previous_head = head.hex()
    if previous is not None:
        previous_last = int(previous["last_sequence"])
        if previous_last == 0:
            previous_head = CHAIN_SEED.hex()
        if previous_last > expected_last or previous_head != previous["chain_sha256"]:
            raise IntegrityFailure("signed journal history changed")
    return {
        "kind": "service-authority",
        "subject_id": None,
        "last_sequence": expected_last,
        "chain_sha256": head.hex(),
    }


def inspect_database(
    path: Path, logical_id: str, previous: dict[str, Any] | None = None
) -> dict[str, Any]:
    connection = open_database(path)
    try:
        previous_streams = (
            {}
            if previous is None
            else keyed(previous["event_streams"], ("kind", "subject_id"))
        )
        projects = []
        streams = []
        for project_id, state_revision, last_event_sequence in connection.execute(
            "SELECT project_id, state_revision, last_event_sequence "
            "FROM projects ORDER BY project_id"
        ):
            project_id = str(project_id)
            state_revision = int(state_revision)
            last_event_sequence = int(last_event_sequence)
            if state_revision < 0 or last_event_sequence < 0:
                raise IntegrityFailure("negative canonical revision")
            projects.append(
                {
                    "project_id": project_id,
                    "state_revision": state_revision,
                    "last_event_sequence": last_event_sequence,
                }
            )
            streams.append(
                event_stream(
                    connection,
                    "events",
                    "project-audit",
                    project_id,
                    last_event_sequence,
                    "project_id = ?",
                    (project_id,),
                    previous_streams.get(("project-audit", project_id)),
                )
            )

        service_last = int(
            connection.execute(
                "SELECT last_sequence FROM service_security_state WHERE singleton = 1"
            ).fetchone()[0]
        )
        streams.append(
            service_authority_stream(
                connection,
                service_last,
                previous_streams.get(("service-authority", None)),
            )
        )

        snapshots = []
        for project_id, workspace_id, source_revision, fingerprint, payload in connection.execute(
            "SELECT project_id, workspace_id, source_revision, fingerprint, payload "
            "FROM governance_snapshots ORDER BY project_id, workspace_id"
        ):
            project_id = str(project_id)
            workspace_id = str(workspace_id)
            source_revision = int(source_revision)
            payload_bytes = bytes(payload)
            payload_sha256 = sha256_hex(payload_bytes)
            if fingerprint != snapshot_fingerprint(
                project_id, workspace_id, source_revision, payload_bytes
            ):
                raise IntegrityFailure("governance snapshot fingerprint mismatch")
            snapshots.append(
                {
                    "project_id": project_id,
                    "workspace_id": workspace_id,
                    "source_revision": source_revision,
                    "payload_sha256": payload_sha256,
                }
            )

        return {
            "logical_id": logical_id,
            "schema_version": SCHEMA_VERSION,
            "semantic_sha256": semantic_database_sha256(connection),
            "projects": projects,
            "event_streams": streams,
            "governance_snapshots": snapshots,
        }
    except (IndexError, TypeError, ValueError, sqlite3.Error) as error:
        raise IntegrityFailure("database contract inspection failed") from error
    finally:
        connection.close()


def discover_databases(state_root: Path) -> list[tuple[Path, str]]:
    require_directory(state_root)
    registry_path = state_root / "registry" / "state" / "state.db"
    registry = open_database(registry_path)
    try:
        registrations = [
            (str(project_id), str(project_key))
            for project_id, project_key in registry.execute(
                "SELECT project_id, project_key FROM projects ORDER BY project_id"
            )
        ]
    except sqlite3.Error as error:
        raise IntegrityFailure("registry inventory failed") from error
    finally:
        registry.close()

    databases = [(registry_path, "registry")]
    expected_paths = set()
    for project_id, project_key in registrations:
        if not project_key or "/" in project_key or project_key in {".", ".."}:
            raise IntegrityFailure("invalid protected project identity")
        path = state_root / "projects" / project_key / "state" / "state.db"
        require_regular_file(path)
        expected_paths.add(path)
        databases.append((path, project_id))

    projects_root = state_root / "projects"
    require_directory(projects_root)
    discovered = set(projects_root.glob("*/state/state.db"))
    if discovered != expected_paths:
        raise IntegrityFailure("protected database inventory mismatch")
    return databases


def public_key_sha256(public_key: Path) -> str:
    require_regular_file(public_key)
    try:
        result = subprocess.run(
            [
                "openssl",
                "pkey",
                "-pubin",
                "-in",
                str(public_key),
                "-outform",
                "DER",
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise IntegrityFailure("authority public key is invalid") from error
    return sha256_hex(result.stdout)


def verify_signature(public_key: Path, checkpoint_payload: bytes, signature: bytes) -> None:
    if len(signature) != 64:
        raise IntegrityFailure("unexpected checkpoint signature size")
    temporary_parent = public_key.parent
    require_directory(temporary_parent)
    with tempfile.TemporaryDirectory(prefix=".verify-", dir=temporary_parent) as temporary_root:
        checkpoint = Path(temporary_root) / "checkpoint.json"
        signature_path = Path(temporary_root) / "checkpoint.sig"
        checkpoint.write_bytes(checkpoint_payload)
        signature_path.write_bytes(signature)
        try:
            subprocess.run(
                [
                    "openssl",
                    "pkeyutl",
                    "-verify",
                    "-pubin",
                    "-inkey",
                    str(public_key),
                    "-rawin",
                    "-in",
                    str(checkpoint),
                    "-sigfile",
                    str(signature_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except (OSError, subprocess.CalledProcessError) as error:
            raise IntegrityFailure("checkpoint signature verification failed") from error


def load_checkpoint(envelope_path: Path, public_key: Path) -> dict[str, Any]:
    require_regular_file(envelope_path)
    try:
        envelope_payload = envelope_path.read_bytes()
        envelope = json.loads(envelope_payload)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise IntegrityFailure("signed checkpoint envelope is invalid") from error
    if canonical_json(envelope) != envelope_payload:
        raise IntegrityFailure("signed checkpoint envelope is not canonical")
    if envelope.get("contract_version") != ENVELOPE_CONTRACT_VERSION:
        raise IntegrityFailure("signed checkpoint envelope is unsupported")
    checkpoint = envelope.get("checkpoint")
    signature_text = envelope.get("signature_base64")
    if not isinstance(checkpoint, dict) or not isinstance(signature_text, str):
        raise IntegrityFailure("signed checkpoint envelope is incomplete")
    try:
        signature = base64.b64decode(signature_text, validate=True)
    except (ValueError, TypeError) as error:
        raise IntegrityFailure("signed checkpoint signature encoding is invalid") from error
    verify_signature(public_key, canonical_json(checkpoint), signature)
    if checkpoint.get("contract_version") != CONTRACT_VERSION:
        raise IntegrityFailure("signed checkpoint contract is unsupported")
    if checkpoint.get("authority_key_sha256") != public_key_sha256(public_key):
        raise IntegrityFailure("signed checkpoint authority changed")
    validate_checkpoint_structure(checkpoint)
    return checkpoint


def valid_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def validate_checkpoint_structure(checkpoint: dict[str, Any]) -> None:
    if set(checkpoint) != {
        "contract_version",
        "epoch",
        "previous_checkpoint_sha256",
        "authority_key_sha256",
        "storage_schema_version",
        "database_count",
        "databases",
    }:
        raise IntegrityFailure("signed checkpoint shape is invalid")
    epoch = checkpoint["epoch"]
    schema_version = checkpoint["storage_schema_version"]
    databases = checkpoint["databases"]
    if type(epoch) is not int or epoch < 1:
        raise IntegrityFailure("signed checkpoint epoch is invalid")
    if schema_version not in SUPPORTED_SIGNED_SCHEMA_VERSIONS:
        raise IntegrityFailure("signed checkpoint storage schema is unsupported")
    previous_digest = checkpoint["previous_checkpoint_sha256"]
    if (epoch == 1 and previous_digest is not None) or (
        epoch > 1 and not valid_sha256(previous_digest)
    ):
        raise IntegrityFailure("signed checkpoint chain link is invalid")
    if not valid_sha256(checkpoint["authority_key_sha256"]):
        raise IntegrityFailure("signed checkpoint authority digest is invalid")
    if not isinstance(databases, list) or type(checkpoint["database_count"]) is not int:
        raise IntegrityFailure("signed checkpoint database inventory is invalid")
    if checkpoint["database_count"] != len(databases):
        raise IntegrityFailure("signed checkpoint database count is invalid")
    for database in databases:
        if not isinstance(database, dict) or set(database) != {
            "logical_id",
            "schema_version",
            "semantic_sha256",
            "projects",
            "event_streams",
            "governance_snapshots",
        }:
            raise IntegrityFailure("signed checkpoint database entry is invalid")
        if database["schema_version"] != schema_version:
            raise IntegrityFailure("signed checkpoint schema inventory is inconsistent")
        if not isinstance(database["logical_id"], str) or not valid_sha256(
            database["semantic_sha256"]
        ):
            raise IntegrityFailure("signed checkpoint database identity is invalid")
        for field in ("projects", "event_streams", "governance_snapshots"):
            if not isinstance(database[field], list):
                raise IntegrityFailure("signed checkpoint database markers are invalid")


def keyed(items: list[dict[str, Any]], fields: tuple[str, ...]) -> dict[tuple[Any, ...], dict[str, Any]]:
    output = {}
    for item in items:
        key = tuple(item.get(field) for field in fields)
        if key in output:
            raise IntegrityFailure("duplicate logical checkpoint entry")
        output[key] = item
    return output


def compare_monotonic(previous: dict[str, Any], current: dict[str, Any]) -> None:
    previous_schema = previous["storage_schema_version"]
    current_schema = current["storage_schema_version"]
    if previous_schema > current_schema:
        raise IntegrityFailure("storage schema rollback detected")
    if previous_schema not in SUPPORTED_SIGNED_SCHEMA_VERSIONS:
        raise IntegrityFailure("signed checkpoint storage schema is unsupported")
    previous_databases = keyed(previous["databases"], ("logical_id",))
    current_databases = keyed(current["databases"], ("logical_id",))
    if not previous_databases.keys() <= current_databases.keys():
        raise IntegrityFailure("protected database rollback detected")

    for database_key, old_database in previous_databases.items():
        new_database = current_databases[database_key]
        if old_database["schema_version"] > new_database["schema_version"]:
            raise IntegrityFailure("storage schema rollback detected")

        old_projects = keyed(old_database["projects"], ("project_id",))
        new_projects = keyed(new_database["projects"], ("project_id",))
        if not old_projects.keys() <= new_projects.keys():
            raise IntegrityFailure("canonical project rollback detected")
        for project_key, old_project in old_projects.items():
            new_project = new_projects[project_key]
            if new_project["state_revision"] < old_project["state_revision"]:
                raise IntegrityFailure("canonical state rollback detected")
            if new_project["last_event_sequence"] < old_project["last_event_sequence"]:
                raise IntegrityFailure("audit sequence rollback detected")

        old_streams = keyed(old_database["event_streams"], ("kind", "subject_id"))
        new_streams = keyed(new_database["event_streams"], ("kind", "subject_id"))
        if not old_streams.keys() <= new_streams.keys():
            raise IntegrityFailure("signed journal stream disappeared")
        for stream_key, old_stream in old_streams.items():
            new_stream = new_streams[stream_key]
            if new_stream["last_sequence"] < old_stream["last_sequence"]:
                raise IntegrityFailure("signed journal rollback detected")
            if new_stream["last_sequence"] == old_stream["last_sequence"]:
                if new_stream["chain_sha256"] != old_stream["chain_sha256"]:
                    raise IntegrityFailure("signed journal history changed")

        old_snapshots = keyed(
            old_database["governance_snapshots"], ("project_id", "workspace_id")
        )
        new_snapshots = keyed(
            new_database["governance_snapshots"], ("project_id", "workspace_id")
        )
        if not old_snapshots.keys() <= new_snapshots.keys():
            raise IntegrityFailure("signed governance snapshot disappeared")
        for snapshot_key, old_snapshot in old_snapshots.items():
            new_snapshot = new_snapshots[snapshot_key]
            if new_snapshot["source_revision"] < old_snapshot["source_revision"]:
                raise IntegrityFailure("governance snapshot rollback detected")
            if (
                new_snapshot["source_revision"] == old_snapshot["source_revision"]
                and new_snapshot["payload_sha256"] != old_snapshot["payload_sha256"]
            ):
                raise IntegrityFailure("signed governance snapshot changed")

        old_markers = (
            old_database["projects"],
            old_database["event_streams"],
            old_database["governance_snapshots"],
        )
        new_markers = (
            new_database["projects"],
            new_database["event_streams"],
            new_database["governance_snapshots"],
        )
        if (
            old_database["schema_version"] == new_database["schema_version"]
            and old_markers == new_markers
            and old_database["semantic_sha256"] != new_database["semantic_sha256"]
        ):
            raise IntegrityFailure("protected state changed without an authoritative sequence")


def build_checkpoint(
    state_root: Path, public_key: Path, previous: dict[str, Any] | None
) -> dict[str, Any]:
    previous_databases = (
        {} if previous is None else keyed(previous["databases"], ("logical_id",))
    )
    databases = [
        inspect_database(path, logical_id, previous_databases.get((logical_id,)))
        for path, logical_id in discover_databases(state_root)
    ]
    checkpoint = {
        "contract_version": CONTRACT_VERSION,
        "epoch": 1 if previous is None else int(previous["epoch"]) + 1,
        "previous_checkpoint_sha256": None,
        "authority_key_sha256": public_key_sha256(public_key),
        "storage_schema_version": SCHEMA_VERSION,
        "database_count": len(databases),
        "databases": databases,
    }
    if previous is not None:
        checkpoint["previous_checkpoint_sha256"] = sha256_hex(canonical_json(previous))
        compare_monotonic(previous, checkpoint)
    return checkpoint


def atomic_write(path: Path, payload: bytes, mode: int) -> None:
    descriptor, temporary_name = tempfile.mkstemp(prefix=".checkpoint-", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb", closefd=True) as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def sign_checkpoint(private_key: Path, checkpoint_payload: bytes, anchor_root: Path) -> bytes:
    require_regular_file(private_key)
    key_mode = stat.S_IMODE(private_key.lstat().st_mode)
    if key_mode & 0o077:
        raise IntegrityFailure("authority private key permissions are too broad")
    with tempfile.TemporaryDirectory(prefix=".sign-", dir=anchor_root) as temporary_root:
        input_path = Path(temporary_root) / "checkpoint.json"
        signature_path = Path(temporary_root) / "checkpoint.sig"
        input_path.write_bytes(checkpoint_payload)
        try:
            subprocess.run(
                [
                    "openssl",
                    "pkeyutl",
                    "-sign",
                    "-inkey",
                    str(private_key),
                    "-rawin",
                    "-in",
                    str(input_path),
                    "-out",
                    str(signature_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except (OSError, subprocess.CalledProcessError) as error:
            raise IntegrityFailure("checkpoint signing failed") from error
        signature = signature_path.read_bytes()
    if len(signature) != 64:
        raise IntegrityFailure("unexpected checkpoint signature size")
    return signature


def seal(args: argparse.Namespace) -> None:
    state_root = Path(args.state_root)
    anchor_root = Path(args.anchor_root)
    private_key = Path(args.private_key)
    public_key = Path(args.public_key)
    require_directory(anchor_root)
    envelope_path = anchor_root / "checkpoint-envelope.json"
    previous = None
    if envelope_path.exists():
        previous = load_checkpoint(envelope_path, public_key)
    checkpoint = build_checkpoint(state_root, public_key, previous)
    payload = canonical_json(checkpoint)
    signature = sign_checkpoint(private_key, payload, anchor_root)
    envelope = {
        "contract_version": ENVELOPE_CONTRACT_VERSION,
        "checkpoint": checkpoint,
        "signature_base64": base64.b64encode(signature).decode("ascii"),
    }
    atomic_write(envelope_path, canonical_json(envelope), 0o600)
    load_checkpoint(envelope_path, public_key)
    print(
        f"contract={CONTRACT_VERSION}\nepoch={checkpoint['epoch']}\n"
        f"database_count={checkpoint['database_count']}\nvalidation=pass"
    )


def verify(args: argparse.Namespace) -> None:
    state_root = Path(args.state_root)
    anchor_root = Path(args.anchor_root)
    public_key = Path(args.public_key)
    require_directory(anchor_root)
    envelope_path = anchor_root / "checkpoint-envelope.json"
    signed = load_checkpoint(envelope_path, public_key)
    current = build_checkpoint(state_root, public_key, None)
    current["epoch"] = signed["epoch"]
    current["previous_checkpoint_sha256"] = signed["previous_checkpoint_sha256"]
    if current != signed:
        compare_monotonic(signed, current)
        raise IntegrityFailure("protected state has unsigned changes")
    print(
        f"contract={CONTRACT_VERSION}\nepoch={signed['epoch']}\n"
        f"database_count={signed['database_count']}\nvalidation=pass"
    )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("mode", choices=("seal", "verify"))
    parser.add_argument("--state-root", required=True)
    parser.add_argument("--anchor-root", required=True)
    parser.add_argument("--public-key", required=True)
    parser.add_argument("--private-key")
    arguments = parser.parse_args()
    if arguments.mode == "seal" and not arguments.private_key:
        raise IntegrityFailure("sealing requires the external private key")
    if arguments.mode == "verify" and arguments.private_key:
        raise IntegrityFailure("verification must not receive the private key")
    return arguments


def main() -> int:
    try:
        arguments = parse_arguments()
        if arguments.mode == "seal":
            seal(arguments)
        else:
            verify(arguments)
        return 0
    except IntegrityFailure as error:
        print(f"integrity-authority: {error}", file=sys.stderr)
        return 1
    except Exception:
        print("integrity-authority: internal integrity inspection failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
