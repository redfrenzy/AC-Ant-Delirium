#!/usr/bin/env python3
"""Evaluate and sign the provider-neutral protected branch gate."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from typing import Any


POLICY_CONTRACT = "protected-branch-policy-v1"
ATTESTATION_CONTRACT = "protected-branch-attestation-v1"
ENVELOPE_CONTRACT = "signed-protected-branch-attestation-v1"
TASK_ATTESTATION_CONTRACT = "protected-task-gate-attestation-v1"
TASK_ENVELOPE_CONTRACT = "signed-protected-task-gate-attestation-v1"
INTEGRITY_ENVELOPE_CONTRACT = "signed-integrity-checkpoint-v1"
INTEGRITY_CONTRACT = "integrity-checkpoint-v1"
SHA256_KEYS = {"wrapper_sha256", "source_sha256"}


class GateFailure(Exception):
    pass


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value, ensure_ascii=True, allow_nan=False, sort_keys=True, separators=(",", ":")
    ).encode("ascii") + b"\n"


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def require_regular(path: Path) -> None:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise GateFailure("required gate input is unavailable") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise GateFailure("unsafe gate input metadata")


def read_json(path: Path) -> tuple[Any, bytes]:
    require_regular(path)
    try:
        payload = path.read_bytes()
        return json.loads(payload), payload
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GateFailure("invalid gate input") from error


def exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        raise GateFailure(f"invalid {label} contract")
    return value


def valid_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def valid_opaque_id(value: Any, prefix: str) -> bool:
    alphabet = set("0123456789abcdefghjkmnpqrstvwxyz")
    return (
        isinstance(value, str)
        and value.startswith(prefix)
        and len(value) == len(prefix) + 26
        and all(character in alphabet for character in value[len(prefix):])
    )


def load_policy(path: Path) -> tuple[dict[str, Any], bytes]:
    value, payload = read_json(path)
    policy = exact_keys(
        value,
        {
            "contract_version",
            "policy_id",
            "enforcement_mode",
            "required_status_context",
            "relevant_branches",
            "compiler_result_precedence",
            "wrapper_rollout",
            "required_proofs",
            "blocking_violation_codes",
            "advisory_observation_codes",
        },
        "policy",
    )
    constants = {
        "contract_version": POLICY_CONTRACT,
        "policy_id": "antdelirium-production-v1",
        "enforcement_mode": "fail-closed-external",
        "required_status_context": "antdelirium/protected-gate",
        "relevant_branches": ["main", "master"],
        "compiler_result_precedence": "preserved",
        "wrapper_rollout": "audit-only-local",
        "required_proofs": [
            "externally-signed-integrity-checkpoint",
            "protected-wrapper-digest",
            "clean-primary-compilation",
        ],
    }
    for key, expected in constants.items():
        if policy[key] != expected:
            raise GateFailure("invalid policy contract")
    for key in ("blocking_violation_codes", "advisory_observation_codes"):
        codes = policy[key]
        if (
            not isinstance(codes, list)
            or not codes
            or codes != sorted(set(codes))
            or any(not isinstance(code, str) or not code for code in codes)
        ):
            raise GateFailure("invalid policy code set")
    if set(policy["blocking_violation_codes"]) & set(policy["advisory_observation_codes"]):
        raise GateFailure("overlapping policy code sets")
    return policy, payload


def load_mechanical_evidence(path: Path) -> tuple[dict[str, Any], bytes]:
    value, payload = read_json(path)
    evidence = exact_keys(
        value,
        {
            "schema_version",
            "result",
            "authority_mode",
            "wrapper_sha256",
            "source_sha256",
            "clean_target",
            "primary_wrapper_participation",
            "local_wrapper_override",
            "cached_followup",
            "signature",
        },
        "mechanical evidence",
    )
    expected = {
        "schema_version": "ci-gate-evidence-v1",
        "result": "passed",
        "authority_mode": "protected-digest",
        "clean_target": True,
        "primary_wrapper_participation": "verified",
        "local_wrapper_override": "superseded",
        "cached_followup": "passed-without-primary-recompile",
        "signature": "none-local-mechanics-only",
    }
    if any(evidence.get(key) != expected_value for key, expected_value in expected.items()):
        raise GateFailure("mechanical evidence is not protected")
    if any(not valid_sha256(evidence.get(key)) for key in SHA256_KEYS):
        raise GateFailure("mechanical evidence digest is invalid")
    return evidence, payload


def public_key_sha256(public_key: Path) -> str:
    require_regular(public_key)
    try:
        result = subprocess.run(
            ["openssl", "pkey", "-pubin", "-in", str(public_key), "-outform", "DER"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise GateFailure("invalid authority public key") from error
    return sha256_hex(result.stdout)


def openssl_verify(public_key: Path, payload: bytes, signature: bytes) -> None:
    require_regular(public_key)
    if len(signature) != 64:
        raise GateFailure("invalid authority signature")
    with tempfile.TemporaryDirectory(prefix=".branch-verify-") as temporary_root:
        input_path = Path(temporary_root) / "payload.json"
        signature_path = Path(temporary_root) / "payload.sig"
        input_path.write_bytes(payload)
        signature_path.write_bytes(signature)
        try:
            subprocess.run(
                [
                    "openssl", "pkeyutl", "-verify", "-pubin", "-inkey",
                    str(public_key), "-rawin", "-in", str(input_path),
                    "-sigfile", str(signature_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except (OSError, subprocess.CalledProcessError) as error:
            raise GateFailure("authority signature verification failed") from error


def decode_signature(value: Any) -> bytes:
    if not isinstance(value, str):
        raise GateFailure("invalid authority signature")
    try:
        return base64.b64decode(value, validate=True)
    except (ValueError, base64.binascii.Error) as error:
        raise GateFailure("invalid authority signature") from error


def load_integrity_checkpoint(path: Path, public_key: Path) -> tuple[dict[str, Any], bytes]:
    value, envelope_payload = read_json(path)
    envelope = exact_keys(
        value, {"contract_version", "checkpoint", "signature_base64"}, "integrity envelope"
    )
    if envelope["contract_version"] != INTEGRITY_ENVELOPE_CONTRACT:
        raise GateFailure("invalid integrity envelope contract")
    checkpoint = envelope["checkpoint"]
    if not isinstance(checkpoint, dict):
        raise GateFailure("invalid integrity checkpoint contract")
    if checkpoint.get("contract_version") != INTEGRITY_CONTRACT:
        raise GateFailure("invalid integrity checkpoint contract")
    exact_keys(
        checkpoint,
        {
            "contract_version",
            "epoch",
            "previous_checkpoint_sha256",
            "authority_key_sha256",
            "storage_schema_version",
            "database_count",
            "databases",
        },
        "integrity checkpoint",
    )
    epoch = checkpoint.get("epoch")
    if not isinstance(epoch, int) or isinstance(epoch, bool) or epoch < 1:
        raise GateFailure("invalid integrity checkpoint epoch")
    if checkpoint.get("authority_key_sha256") != public_key_sha256(public_key):
        raise GateFailure("integrity authority key differs")
    if checkpoint.get("storage_schema_version") != 13:
        raise GateFailure("invalid integrity checkpoint schema")
    previous = checkpoint.get("previous_checkpoint_sha256")
    if previous is not None and not valid_sha256(previous):
        raise GateFailure("invalid previous checkpoint digest")
    databases = checkpoint.get("databases")
    if (
        not isinstance(databases, list)
        or not databases
        or checkpoint.get("database_count") != len(databases)
    ):
        raise GateFailure("invalid integrity database inventory")
    logical_ids: set[str] = set()
    for database in databases:
        exact_keys(
            database,
            {
                "logical_id", "schema_version", "semantic_sha256", "projects",
                "event_streams", "governance_snapshots",
            },
            "integrity database",
        )
        logical_id = database["logical_id"]
        if (
            not isinstance(logical_id, str)
            or not logical_id
            or logical_id in logical_ids
            or database["schema_version"] != 13
            or not valid_sha256(database["semantic_sha256"])
            or any(not isinstance(database[key], list) for key in (
                "projects", "event_streams", "governance_snapshots"
            ))
        ):
            raise GateFailure("invalid integrity database contract")
        logical_ids.add(logical_id)
    openssl_verify(
        public_key,
        canonical_json(checkpoint),
        decode_signature(envelope["signature_base64"]),
    )
    return checkpoint, envelope_payload


def openssl_sign(private_key: Path, payload: bytes) -> bytes:
    require_regular(private_key)
    if stat.S_IMODE(private_key.lstat().st_mode) & 0o077:
        raise GateFailure("authority private key permissions are too broad")
    with tempfile.TemporaryDirectory(prefix=".branch-sign-") as temporary_root:
        input_path = Path(temporary_root) / "payload.json"
        signature_path = Path(temporary_root) / "payload.sig"
        input_path.write_bytes(payload)
        try:
            subprocess.run(
                [
                    "openssl", "pkeyutl", "-sign", "-inkey", str(private_key),
                    "-rawin", "-in", str(input_path), "-out", str(signature_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            signature = signature_path.read_bytes()
        except (OSError, subprocess.CalledProcessError) as error:
            raise GateFailure("attestation signing failed") from error
    if len(signature) != 64:
        raise GateFailure("invalid generated attestation signature")
    return signature


def atomic_write(path: Path, payload: bytes) -> None:
    if path.exists() and path.is_symlink():
        raise GateFailure("unsafe attestation output metadata")
    parent = path.parent
    if not parent.is_dir() or parent.is_symlink():
        raise GateFailure("unsafe attestation output directory")
    descriptor, temporary_name = tempfile.mkstemp(prefix=".branch-attestation-", dir=parent)
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb", closefd=True) as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def evaluate(args: argparse.Namespace) -> int:
    policy, policy_payload = load_policy(Path(args.policy))
    evidence, evidence_payload = load_mechanical_evidence(Path(args.mechanical_evidence))
    checkpoint, integrity_payload = load_integrity_checkpoint(
        Path(args.integrity_envelope), Path(args.integrity_public_key)
    )
    blocking = sorted(set(args.violation))
    advisory = sorted(set(args.observation))
    if any(code not in policy["blocking_violation_codes"] for code in blocking):
        raise GateFailure("undefined blocking violation code")
    if any(code not in policy["advisory_observation_codes"] for code in advisory):
        raise GateFailure("undefined advisory observation code")
    attestation = {
        "contract_version": ATTESTATION_CONTRACT,
        "decision": "blocked" if blocking else "passed",
        "policy_sha256": sha256_hex(policy_payload),
        "mechanical_evidence_sha256": sha256_hex(evidence_payload),
        "integrity_envelope_sha256": sha256_hex(integrity_payload),
        "source_sha256": evidence["source_sha256"],
        "wrapper_sha256": evidence["wrapper_sha256"],
        "checkpoint_epoch": checkpoint["epoch"],
        "required_status_context": policy["required_status_context"],
        "blocking_violations": blocking,
        "advisory_observations": advisory,
        "compiler_result_precedence": policy["compiler_result_precedence"],
    }
    signature = openssl_sign(Path(args.attestation_private_key), canonical_json(attestation))
    envelope = {
        "contract_version": ENVELOPE_CONTRACT,
        "attestation": attestation,
        "signature_base64": base64.b64encode(signature).decode("ascii"),
    }
    atomic_write(Path(args.output), canonical_json(envelope))
    print(f"decision={attestation['decision']}")
    print(f"blocking_violation_count={len(blocking)}")
    print(f"advisory_observation_count={len(advisory)}")
    return 1 if blocking else 0


def verify(args: argparse.Namespace) -> int:
    policy, policy_payload = load_policy(Path(args.policy))
    value, _ = read_json(Path(args.envelope))
    envelope = exact_keys(
        value, {"contract_version", "attestation", "signature_base64"}, "attestation envelope"
    )
    if envelope["contract_version"] != ENVELOPE_CONTRACT:
        raise GateFailure("invalid attestation envelope contract")
    attestation = envelope["attestation"]
    required_keys = {
        "contract_version", "decision", "policy_sha256", "mechanical_evidence_sha256",
        "integrity_envelope_sha256", "source_sha256", "wrapper_sha256", "checkpoint_epoch",
        "required_status_context", "blocking_violations", "advisory_observations",
        "compiler_result_precedence",
    }
    exact_keys(attestation, required_keys, "attestation")
    if (
        attestation["contract_version"] != ATTESTATION_CONTRACT
        or attestation["policy_sha256"] != sha256_hex(policy_payload)
        or attestation["required_status_context"] != policy["required_status_context"]
        or attestation["compiler_result_precedence"] != "preserved"
        or any(not valid_sha256(attestation.get(key)) for key in (
            "policy_sha256", "mechanical_evidence_sha256", "integrity_envelope_sha256",
            "source_sha256", "wrapper_sha256",
        ))
        or not isinstance(attestation["checkpoint_epoch"], int)
        or isinstance(attestation["checkpoint_epoch"], bool)
        or attestation["checkpoint_epoch"] < 1
    ):
        raise GateFailure("invalid attestation contract")
    blocking = attestation["blocking_violations"]
    advisory = attestation["advisory_observations"]
    if (
        not isinstance(blocking, list) or blocking != sorted(set(blocking))
        or not isinstance(advisory, list) or advisory != sorted(set(advisory))
        or any(code not in policy["blocking_violation_codes"] for code in blocking)
        or any(code not in policy["advisory_observation_codes"] for code in advisory)
        or attestation["decision"] != ("blocked" if blocking else "passed")
    ):
        raise GateFailure("invalid attestation decision")
    openssl_verify(
        Path(args.attestation_public_key),
        canonical_json(attestation),
        decode_signature(envelope["signature_base64"]),
    )
    print(f"decision={attestation['decision']}")
    return 1 if blocking else 0


def evaluate_task(args: argparse.Namespace) -> int:
    policy, policy_payload = load_policy(Path(args.policy))
    evidence, evidence_payload = load_mechanical_evidence(Path(args.mechanical_evidence))
    checkpoint, integrity_payload = load_integrity_checkpoint(
        Path(args.integrity_envelope), Path(args.integrity_public_key)
    )
    for value, prefix in (
        (args.project_id, "prj_"),
        (args.workspace_id, "wsp_"),
        (args.task_id, "tsk_"),
        (args.review_id, "rev_"),
    ):
        if not valid_opaque_id(value, prefix):
            raise GateFailure("invalid task gate binding")
    if args.task_revision < 1 or args.state_revision < 0:
        raise GateFailure("invalid task gate revision")
    if evidence["source_sha256"] != args.source_fingerprint:
        raise GateFailure("protected compilation source differs from reviewed source")
    attestation = {
        "blocking_violations": [],
        "checkpoint_epoch": checkpoint["epoch"],
        "contract_version": TASK_ATTESTATION_CONTRACT,
        "decision": "passed",
        "integrity_envelope_sha256": sha256_hex(integrity_payload),
        "mechanical_evidence_sha256": sha256_hex(evidence_payload),
        "policy_sha256": sha256_hex(policy_payload),
        "project_id": args.project_id,
        "review_id": args.review_id,
        "source_fingerprint": args.source_fingerprint,
        "state_revision": args.state_revision,
        "task_id": args.task_id,
        "task_revision": args.task_revision,
        "workspace_id": args.workspace_id,
        "wrapper_sha256": evidence["wrapper_sha256"],
    }
    signature = openssl_sign(Path(args.attestation_private_key), canonical_json(attestation))
    atomic_write(
        Path(args.output),
        canonical_json({
            "attestation": attestation,
            "contract_version": TASK_ENVELOPE_CONTRACT,
            "signature_base64": base64.b64encode(signature).decode("ascii"),
        }),
    )
    print("decision=passed")
    print(f"checkpoint_epoch={checkpoint['epoch']}")
    return 0


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=False)
    subparsers = parser.add_subparsers(dest="mode", required=True)
    evaluate_parser = subparsers.add_parser("evaluate", add_help=False)
    evaluate_parser.add_argument("--policy", required=True)
    evaluate_parser.add_argument("--mechanical-evidence", required=True)
    evaluate_parser.add_argument("--integrity-envelope", required=True)
    evaluate_parser.add_argument("--integrity-public-key", required=True)
    evaluate_parser.add_argument("--attestation-private-key", required=True)
    evaluate_parser.add_argument("--output", required=True)
    evaluate_parser.add_argument("--violation", action="append", default=[])
    evaluate_parser.add_argument("--observation", action="append", default=[])
    verify_parser = subparsers.add_parser("verify", add_help=False)
    verify_parser.add_argument("--policy", required=True)
    verify_parser.add_argument("--envelope", required=True)
    verify_parser.add_argument("--attestation-public-key", required=True)
    task_parser = subparsers.add_parser("task-evaluate", add_help=False)
    task_parser.add_argument("--policy", required=True)
    task_parser.add_argument("--mechanical-evidence", required=True)
    task_parser.add_argument("--integrity-envelope", required=True)
    task_parser.add_argument("--integrity-public-key", required=True)
    task_parser.add_argument("--attestation-private-key", required=True)
    task_parser.add_argument("--output", required=True)
    task_parser.add_argument("--project-id", required=True)
    task_parser.add_argument("--workspace-id", required=True)
    task_parser.add_argument("--task-id", required=True)
    task_parser.add_argument("--task-revision", required=True, type=int)
    task_parser.add_argument("--state-revision", required=True, type=int)
    task_parser.add_argument("--review-id", required=True)
    task_parser.add_argument("--source-fingerprint", required=True)
    return parser.parse_args()


def main() -> int:
    try:
        arguments = parse_arguments()
        if arguments.mode == "evaluate":
            return evaluate(arguments)
        if arguments.mode == "task-evaluate":
            return evaluate_task(arguments)
        return verify(arguments)
    except GateFailure as error:
        print(f"protected-branch-authority: {error}", file=sys.stderr)
        return 2
    except Exception:
        print("protected-branch-authority: internal gate failure", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
