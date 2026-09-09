#!/usr/bin/env python3
"""One scoped operation of the official task API; never an agent entrypoint.

No caller-selected executable, socket, credential path, shell code or .env file.
The Rust client remains authoritative for the complete request/receipt contract.
"""

from __future__ import annotations

import contextlib
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import secrets
import signal
import stat
import subprocess
import sys
from dataclasses import dataclass


REPOSITORY = Path(__file__).resolve().parents[2]
BINARY = "/usr/libexec/antdelirium/antdelirium"
SOCKET = "/run/antdelirium-admin/admin.sock"
CREDENTIAL = "/var/lib/antdelirium-operator/installation.cred"
MAX_BYTES = 4 * 1024 * 1024 - 64 * 1024
TTL = 600
TIMEOUT = 180
PREFIX = "ANTDELIRIUM_"
# action: input contract, actor, capability operation, binary command family
OPERATIONS = {
    "prepare": ("task-generation-source-v1", "operator", "state.admin", "task-package"),
    "inspect": ("task-package-inspect-v1", "operator", "diagnostic.private", "task-package"),
    "attest": ("task-package-attestation-v1", "ci", "attestation.create", "task-package"),
    "approve": ("task-package-approval-v1", "operator", "state.admin", "task-package"),
    "install": ("governance-task-package-v2", "ci", "installation.admin", "task-package"),
    "identity-associate": ("migration-identity-association-v1", "operator", "state.admin", "migration"),
    "identity-rollback": ("migration-identity-rollback-v1", "operator", "state.admin", "migration"),
}


class Refused(Exception):
    """Only fixed, non-sensitive reason codes may leave this module."""


@dataclass(frozen=True)
class Subject:
    uid: int
    gid: int


def require(condition, code):
    if not condition:
        raise Refused(code)


def opaque(value, prefix):
    return isinstance(value, str) and re.fullmatch(prefix + r"[0-9a-hjkmnp-tv-z]{26}", value) is not None


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "duplicate-json-key")
        value[key] = item
    return value


def parse_json(data, maximum=MAX_BYTES):
    require(0 < len(data) <= maximum, "json-size")
    try:
        value = json.loads(data, object_pairs_hook=unique_object,
                           parse_constant=lambda _: (_ for _ in ()).throw(Refused("json-number")))
    except (ValueError, UnicodeError, RecursionError):
        raise Refused("invalid-json") from None
    require(isinstance(value, dict), "json-object-required")
    return value


def subject_digest(uid):
    return hashlib.sha256(b"task-package-subject-v1\0" + uid.to_bytes(4, "big")).hexdigest()


def request_scope(action, value, operator, subject):
    require(action in OPERATIONS, "unknown-operation")
    contract, actor, _, family = OPERATIONS[action]
    require(value.get("contract_version") == contract, "wrong-contract")
    require(operator.uid > 0 and subject.uid > 0, "root-subject-forbidden")
    require((actor == "operator" and subject == operator)
            or (actor == "ci" and subject.uid != operator.uid), "separate-subject-required")
    project = value.get("candidate_project_id" if family == "migration" else "project_id")
    workspace = value.get("candidate_workspace_id" if family == "migration" else "workspace_id")
    require(opaque(project, "prj_") and opaque(workspace, "wsp_"), "invalid-exact-scope")
    if action == "attest":
        require(value.get("verifier_subject_sha256") == subject_digest(subject.uid), "verifier-identity-mismatch")
    if action == "approve":
        require(value.get("operator_subject_sha256") == subject_digest(operator.uid), "operator-identity-mismatch")
    return project, workspace


def validate_capability(data):
    value = parse_json(data, 4096)
    require(set(value) == {"contract_version", "capability"}
            and value["contract_version"] == "admin-capability-v1", "invalid-capability")
    cap = value["capability"]
    require(isinstance(cap, dict) and set(cap) == {"capability_id", "token"}, "invalid-capability")
    require(opaque(cap["capability_id"], "cap_") and isinstance(cap["token"], str)
            and re.fullmatch(r"[A-Za-z0-9_-]{43}", cap["token"]) is not None, "invalid-capability")
    return cap["capability_id"]


@contextlib.contextmanager
def sealed_input(data):
    descriptor = os.memfd_create("antdelirium-input", os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
    reader = None
    try:
        write_all(descriptor, data)
        os.fchmod(descriptor, 0o600)
        fcntl.fcntl(descriptor, fcntl.F_ADD_SEALS,
                    fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL)
        reader = os.open(f"/proc/self/fd/{descriptor}", os.O_RDONLY | os.O_CLOEXEC)
        yield reader
    finally:
        if reader is not None:
            os.close(reader)
        os.close(descriptor)


def write_all(descriptor, data):
    remaining = memoryview(data)
    while remaining:
        count = os.write(descriptor, remaining)
        require(count > 0, "short-write")
        remaining = remaining[count:]


@contextlib.contextmanager
def finishing_revocation():
    previous = {number: signal.signal(number, signal.SIG_IGN) for number in (signal.SIGINT, signal.SIGTERM)}
    try:
        yield
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


def private_output(directory_fd, name, subject):
    fd = os.open(name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC,
                 0o600, dir_fd=directory_fd)
    try:
        os.fchown(fd, subject.uid, subject.gid)
        os.fchmod(fd, 0o600)
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size == 0, "unsafe-private-output")
        return fd
    except BaseException:
        os.close(fd)
        raise


class InstalledClient:
    def invoke(self, subject, args, bindings):
        # Only the explicitly bound descriptors and a clean environment cross exec.
        environment = {"PATH": "/usr/bin:/bin", "LANG": "C", PREFIX + "ADMIN_SOCKET": SOCKET}
        environment.update({PREFIX + key: str(value) for key, value in bindings.items()})
        descriptors = tuple(sorted({value for key, value in bindings.items() if key.endswith("_FD")}))
        command = ["/usr/bin/setpriv", f"--reuid={subject.uid}", f"--regid={subject.gid}",
                   "--init-groups", "--no-new-privs", "--inh-caps=-all", "--ambient-caps=-all",
                   "--bounding-set=-all", BINARY, *args]
        try:
            return subprocess.run(command, env=environment, cwd="/", stdin=subprocess.DEVNULL,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  close_fds=True, pass_fds=descriptors, timeout=TIMEOUT,
                                  start_new_session=True, check=False).returncode
        except subprocess.TimeoutExpired:
            return 124


def validate_result(action, data, project, workspace, request):
    value = parse_json(data)
    receipt = value.get("receipt") if action in ("prepare", "inspect") else value
    require(isinstance(receipt, dict), "invalid-receipt")
    if action == "identity-rollback":
        require(receipt.get("contract_version") == "migration-identity-rollback-receipt-v1"
                and receipt.get("state") == "rolled_back"
                and receipt.get("preserved_candidate_project_id") == project
                and receipt.get("preserved_candidate_workspace_id") == workspace
                and receipt.get("project_id") == request.get("provisional_project_id")
                and receipt.get("workspace_id") == request.get("provisional_workspace_id"), "receipt-scope")
    else:
        contract = ("migration-identity-association-receipt-v1" if action == "identity-associate"
                    else "task-package-operation-receipt-v1")
        require(receipt.get("contract_version") == contract and receipt.get("project_id") == project
                and receipt.get("workspace_id") == workspace, "receipt-scope")
        if action != "identity-associate":
            require(receipt.get("operation") == action, "receipt-operation")
    # Full digest/revision/decision binding is checked by the Rust client before writing this FD.
    return receipt


def execute(action, data, operator, subject, private_fd, manager, client, summary):
    request = parse_json(data)
    project, workspace = request_scope(action, request, operator, subject)
    validate_capability(manager)
    summary.update(project_id=project, workspace_id=workspace)
    grant_fd = private_output(private_fd, "grant.json", operator)
    output_fd = None
    issued = None
    result = None
    try:
        output_fd = private_output(private_fd, "receipt.json", subject)
        with sealed_input(manager) as admin:
            summary["revocation"] = "unconfirmed"
            status = client.invoke(operator, ["capability", "grant"], {
                "ADMIN_CAPABILITY_FD": admin, "CAPABILITY_OUTPUT_FD": grant_fd,
                "CAPABILITY_SUBJECT_UID": subject.uid, "CAPABILITY_ACTOR": OPERATIONS[action][1],
                "CAPABILITY_CHANNEL": "administrative", "CAPABILITY_OPERATION": OPERATIONS[action][2],
                "CAPABILITY_PROJECT_ID": project, "CAPABILITY_WORKSPACE_ID": workspace,
                "CAPABILITY_LIFETIME_SECONDS": TTL, "CAPABILITY_REVOCATION_EPOCH": 0,
            })
        summary["grant_exit"] = status
        require(status == 0, "grant-failed")
        issued = os.pread(grant_fd, 4097, 0)
        summary["capability_id"] = validate_capability(issued)
        family = "MIGRATION_ASSOCIATION" if OPERATIONS[action][3] == "migration" else "TASK_PACKAGE"
        with sealed_input(data) as source, sealed_input(issued) as capability:
            status = client.invoke(subject, ["operator", OPERATIONS[action][3], action], {
                "ADMIN_CAPABILITY_FD": capability, family + "_INPUT_FD": source,
                family + "_OUTPUT_FD": output_fd,
            })
        summary["operation_exit"] = status
        require(status == 0, "operation-failed")
        result = os.pread(output_fd, MAX_BYTES + 1, 0)
        validate_result(action, result, project, workspace, request)
    finally:
        # The client writes the grant descriptor BEFORE sending the request. Even
        # a failed/timed-out grant may have reached the daemon and needs revocation.
        try:
            issued = os.pread(grant_fd, 4097, 0)
            if issued:
                summary["capability_id"] = validate_capability(issued)
                summary["revocation"] = "pending"
                with finishing_revocation(), sealed_input(manager) as admin, sealed_input(issued) as target:
                    status = client.invoke(operator, ["capability", "revoke"], {
                        "ADMIN_CAPABILITY_FD": admin, "CAPABILITY_TARGET_FD": target,
                        "CAPABILITY_REVOCATION_EPOCH": 0,
                    })
                summary["revoke_exit"] = status
                summary["revocation"] = "pass" if status == 0 else "failed"
                require(status == 0, "revocation-failed")
        except BaseException:
            if summary["revocation"] != "failed":
                summary["revocation"] = "unconfirmed"
            raise
        finally:
            # Retain the owned fixture directory and receipt, but no plaintext token.
            os.ftruncate(grant_fd, 0)
            os.fsync(grant_fd)
            os.close(grant_fd)
            if output_fd is not None:
                os.close(output_fd)
    return result


def checked_stdio(operator):
    for descriptor in (0, 1):
        info = os.fstat(descriptor)
        flags = fcntl.fcntl(descriptor, fcntl.F_GETFL)
        require(stat.S_ISREG(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o600
                and info.st_uid == operator.uid and info.st_nlink == 1, "private-redirection-required")
        require(os.lseek(descriptor, 0, os.SEEK_CUR) == 0, "descriptor-offset")
        if descriptor == 0:
            require(flags & os.O_ACCMODE == os.O_RDONLY and 0 < info.st_size <= MAX_BYTES, "input-descriptor")
        else:
            require(flags & os.O_ACCMODE != os.O_RDONLY and info.st_size == 0, "output-descriptor")
    data = bytearray()
    while len(data) <= MAX_BYTES:
        chunk = os.read(0, min(65536, MAX_BYTES + 1 - len(data)))
        if not chunk:
            break
        data.extend(chunk)
    require(len(data) == os.fstat(0).st_size and len(data) <= MAX_BYTES, "input-changed-or-oversized")
    return bytes(data)


def new_log_run(operator):
    root = os.open(REPOSITORY, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        for name in ("target", "t118-official-task-operation-logs"):
            created = False
            try:
                os.mkdir(name, 0o755, dir_fd=root)
                created = True
            except FileExistsError:
                pass
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=root)
            os.close(root)
            root = child
            info = os.fstat(root)
            require(info.st_uid in (0, operator.uid) and info.st_mode & 0o002 == 0, "unsafe-log-parent")
            if created:
                os.fchmod(root, 0o755)
        run_id = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + secrets.token_hex(8)
        os.mkdir(run_id, 0o755, dir_fd=root)
        run_fd = os.open(run_id, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=root)
        require(os.fstat(run_fd).st_uid == os.geteuid(), "unsafe-run-owner")
        os.fchmod(run_fd, 0o755)
        os.mkdir("private", 0o700, dir_fd=run_fd)
        private_fd = os.open("private", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=run_fd)
        return run_id, run_fd, private_fd
    finally:
        os.close(root)


def trusted_file(path):
    info = os.lstat(path)
    require(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and info.st_mode & 0o022 == 0, "unsafe-installed-resource")


def socket_subjects(operator, subject):
    directory = os.lstat(str(Path(SOCKET).parent))
    endpoint = os.lstat(SOCKET)
    require(stat.S_ISDIR(directory.st_mode) and directory.st_uid == 0
            and directory.st_mode & 0o022 == 0 and stat.S_ISSOCK(endpoint.st_mode)
            and endpoint.st_uid == 0 and stat.S_IMODE(endpoint.st_mode) == 0o660,
            "unsafe-admin-socket")
    for identity in (operator, subject):
        account = pwd.getpwuid(identity.uid)
        groups = os.getgrouplist(account.pw_name, identity.gid)
        require(directory.st_gid in groups and endpoint.st_gid in groups, "admin-group-membership-required")


def identities(arguments):
    require(len(arguments) in (1, 3) and arguments[0] in OPERATIONS, "usage-operation-ci-user")
    require(os.geteuid() == 0, "sudo-required")
    operator_user = pwd.getpwnam(os.environ.get("SUDO_USER", ""))
    require(operator_user.pw_uid > 0 and str(operator_user.pw_uid) == os.environ.get("SUDO_UID"), "invalid-sudo-subject")
    operator = Subject(operator_user.pw_uid, operator_user.pw_gid)
    if OPERATIONS[arguments[0]][1] == "ci":
        require(len(arguments) == 3 and arguments[1] == "--ci-user", "explicit-ci-user-required")
        ci = pwd.getpwnam(arguments[2])
        require(ci.pw_uid not in (0, operator.uid, pwd.getpwnam("antdelirium").pw_uid), "separate-ci-user-required")
        subject = Subject(ci.pw_uid, ci.pw_gid)
    else:
        require(len(arguments) == 1, "unexpected-ci-user")
        subject = operator
    return operator, subject


def write_summary(run_fd, name, summary):
    descriptor = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         0o644, dir_fd=run_fd)
    with os.fdopen(descriptor, "wb") as output:
        output.write(json.dumps(summary, sort_keys=True).encode() + b"\n")
        output.flush()
        os.fchmod(output.fileno(), 0o644)
        os.fsync(output.fileno())
    os.fsync(run_fd)


def main(arguments):
    if arguments == ["--help"]:
        print("Usage: run-official-task-operation.sh OPERATION [--ci-user USER]\n"
              "Operations: prepare inspect attest approve install identity-associate identity-rollback\n"
              "Run with sudo from the operator account. attest/install require a distinct CI user.\n"
              "stdin: private regular JSON file, mode 0600, owned by the operator.\n"
              "stdout: new empty private regular receipt file, mode 0600, owned by the operator.\n"
              "Use umask 077 and shell noclobber before redirecting; no JSON/token/path arguments.\n"
              "No installation, account creation or automatic approval is performed.")
        return 0
    run_fd = private_fd = None
    summary = {"contract_version": "official-task-operation-launcher-v1", "physical_locators": "redacted",
               "revocation": "not-issued", "result": "failed"}
    try:
        operator, subject = identities(arguments)
        action = arguments[0]
        summary["operation"] = action
        data = checked_stdio(operator)
        request_scope(action, parse_json(data), operator, subject)
        for path in (BINARY, CREDENTIAL, "/usr/bin/setpriv", "/usr/bin/systemd-creds"):
            trusted_file(path)
        socket_subjects(operator, subject)
        run_id, run_fd, private_fd = new_log_run(operator)
        summary["run_id"] = run_id
        write_summary(run_fd, "started.json", {**summary, "result": "running",
                      "revocation": "unconfirmed", "grant_validity_seconds": TTL})
        # No plaintext credential file, token argument, inherited Python hook or shell evaluation.
        decrypted = subprocess.run(["/usr/bin/systemd-creds", "--name=antdelirium-operator-installation-v1",
                                    "decrypt", CREDENTIAL, "-"], env={"PATH": "/usr/bin:/bin", "LANG": "C"},
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   timeout=30, check=False)
        require(decrypted.returncode == 0, "credential-unavailable")
        result = execute(action, data, operator, subject, private_fd, decrypted.stdout, InstalledClient(), summary)
        write_all(1, result)
        os.fsync(1)
        summary["result"] = "pass"
        return 0
    except Refused as error:
        summary["reason"] = str(error)
        print("AD-T118: " + str(error), file=sys.stderr)
        return 1
    except (OSError, KeyError, ValueError, subprocess.SubprocessError, KeyboardInterrupt):
        summary["reason"] = "operation-interrupted-or-unavailable"
        print("AD-T118: operation-interrupted-or-unavailable", file=sys.stderr)
        return 1
    finally:
        if private_fd is not None:
            os.close(private_fd)
        if run_fd is not None:
            try:
                write_summary(run_fd, "result.json", summary)
                print("AD-T118: logs=target/t118-official-task-operation-logs/" + summary["run_id"], file=sys.stderr)
            except OSError:
                print("AD-T118: result-log-unavailable", file=sys.stderr)
                # Do not claim success when its audit summary could not be saved.
                return 1
            finally:
                os.close(run_fd)


if __name__ == "__main__":
    os.umask(0o077)
    def interrupted(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main(sys.argv[1:]))
