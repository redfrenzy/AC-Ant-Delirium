#!/usr/bin/env python3
"""Rootless launcher tests. Only synthetic bytes and project-local fixtures."""

import fcntl
import io
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1] / "official-task-operation.py"
spec = importlib.util.spec_from_file_location("official_task_operation", SOURCE)
launcher = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = launcher
spec.loader.exec_module(launcher)
OPERATOR = launcher.Subject(os.getuid(), os.getgid())
CI = launcher.Subject(os.getuid() + 1, os.getgid())
PROJECT = "prj_" + "1" * 26
WORKSPACE = "wsp_" + "2" * 26
TOKEN = {"contract_version": "admin-capability-v1", "capability": {
    "capability_id": "cap_" + "3" * 26, "token": "A" * 43,
}}
MANAGER = json.dumps({**TOKEN, "capability": {
    "capability_id": "cap_" + "4" * 26, "token": "B" * 43,
}}).encode()


def request(action):
    value = {"contract_version": launcher.OPERATIONS[action][0], "project_id": PROJECT,
             "workspace_id": WORKSPACE, "request_id": "req_" + "5" * 26}
    if action.startswith("identity-"):
        value.update(candidate_project_id=PROJECT, candidate_workspace_id=WORKSPACE,
                     provisional_project_id="prj_" + "6" * 26, provisional_workspace_id="wsp_" + "7" * 26)
    if action == "attest":
        value["verifier_subject_sha256"] = launcher.subject_digest(CI.uid)
    if action == "approve":
        value["operator_subject_sha256"] = launcher.subject_digest(OPERATOR.uid)
    return value


def receipt(action, value):
    result = {"contract_version": "task-package-operation-receipt-v1",
              "project_id": PROJECT, "workspace_id": WORKSPACE, "operation": action}
    if action == "identity-associate":
        result["contract_version"] = "migration-identity-association-receipt-v1"
    elif action == "identity-rollback":
        result.update(contract_version="migration-identity-rollback-receipt-v1", state="rolled_back",
                      project_id=value["provisional_project_id"], workspace_id=value["provisional_workspace_id"],
                      preserved_candidate_project_id=PROJECT, preserved_candidate_workspace_id=WORKSPACE)
    if action in ("prepare", "inspect"):
        result = {"receipt": result, "proposal": {"private_objective": "NOT-A-LOG"}}
    return json.dumps(result).encode()


class FakeClient:
    def __init__(self, testcase, action, value, fail=None):
        self.testcase, self.action, self.value, self.fail = testcase, action, value, fail
        self.calls = []

    def invoke(self, subject, args, bindings):
        self.calls.append((subject, args, bindings.copy()))
        t = self.testcase
        for key, descriptor in bindings.items():
            if key.endswith("_FD"):
                t.assertGreaterEqual(descriptor, 3)
                t.assertTrue(stat.S_ISREG(os.fstat(descriptor).st_mode))
        authority = os.pread(bindings["ADMIN_CAPABILITY_FD"], 4096, 0)
        if args == ["capability", "grant"]:
            t.assertEqual(subject, OPERATOR)
            t.assertEqual(authority, MANAGER)
            t.assertEqual(bindings["CAPABILITY_PROJECT_ID"], PROJECT)
            t.assertEqual(bindings["CAPABILITY_WORKSPACE_ID"], WORKSPACE)
            t.assertNotIn("CAPABILITY_TASK_ID", bindings)
            t.assertEqual(bindings["CAPABILITY_ACTOR"], launcher.OPERATIONS[self.action][1])
            t.assertEqual(bindings["CAPABILITY_OPERATION"], launcher.OPERATIONS[self.action][2])
            t.assertEqual(bindings["CAPABILITY_LIFETIME_SECONDS"], 600)
            os.write(bindings["CAPABILITY_OUTPUT_FD"], json.dumps(TOKEN).encode())
            return 124 if self.fail == "grant" else 0
        if args == ["capability", "revoke"]:
            t.assertEqual(subject, OPERATOR)
            t.assertEqual(authority, MANAGER)
            t.assertEqual(json.loads(os.pread(bindings["CAPABILITY_TARGET_FD"], 4096, 0)), TOKEN)
            return 69 if self.fail == "revoke" else 0
        t.assertEqual(args, ["operator", launcher.OPERATIONS[self.action][3], self.action])
        t.assertEqual(subject, CI if launcher.OPERATIONS[self.action][1] == "ci" else OPERATOR)
        t.assertEqual(json.loads(authority), TOKEN)
        source = next(fd for key, fd in bindings.items() if key.endswith("INPUT_FD"))
        t.assertEqual(json.loads(os.pread(source, launcher.MAX_BYTES, 0)), self.value)
        t.assertEqual(fcntl.fcntl(source, fcntl.F_GETFL) & os.O_ACCMODE, os.O_RDONLY)
        t.assertTrue(fcntl.fcntl(source, fcntl.F_GET_SEALS) & fcntl.F_SEAL_WRITE)
        if self.fail == "interrupt":
            raise KeyboardInterrupt
        if self.fail == "operation":
            return 78
        result = receipt(self.action, self.value)
        if self.fail == "receipt":
            result = b'{"private":"/outside/project/token"}'
        output = next(fd for key, fd in bindings.items() if key.endswith("OUTPUT_FD"))
        t.assertEqual(stat.S_IMODE(os.fstat(output).st_mode), 0o600)
        t.assertEqual(os.fstat(output).st_nlink, 1)
        os.write(output, result)
        return 0


class LauncherTests(unittest.TestCase):
    def setUp(self):
        parent = launcher.REPOSITORY / "target/t118-launcher-tests"
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="case-", dir=parent))
        self.fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        self.addCleanup(os.close, self.fd)  # Preserve fixture directories; never clean user paths.

    def exercise(self, action, fail=None):
        # Tests assert fchown's intended owner instead of requiring sudo.
        subject = CI if launcher.OPERATIONS[action][1] == "ci" else OPERATOR
        value = request(action)
        client = FakeClient(self, action, value, fail)
        summary = {"revocation": "not-issued"}
        with mock.patch.object(launcher.os, "fchown") as ownership:
            try:
                result = launcher.execute(action, json.dumps(value).encode(), OPERATOR, subject,
                                          self.fd, MANAGER, client, summary)
            finally:
                self.assertEqual(ownership.call_args_list[0].args[1:], (OPERATOR.uid, OPERATOR.gid))
                self.assertEqual(ownership.call_args_list[1].args[1:], (subject.uid, subject.gid))
                self.assertEqual((self.root / "grant.json").read_bytes(), b"")
                self.assertEqual(client.calls[-1][1], ["capability", "revoke"])
                logged = json.dumps(summary)
                self.assertNotIn("A" * 43, logged)
                self.assertNotIn("B" * 43, logged)
                self.assertNotIn("NOT-A-LOG", logged)
                self.assertNotIn("/outside/", logged)
        self.assertEqual(summary["revocation"], "pass")
        self.assertEqual(result, receipt(action, value))
        return client

    def test_every_action_uses_exact_authority_private_fds_and_revocation(self):
        for action in launcher.OPERATIONS:
            with self.subTest(action=action):
                folder = self.root / action
                folder.mkdir()
                with context_fd(folder) as descriptor:
                    original_fd, original_root = self.fd, self.root
                    self.fd, self.root = descriptor, folder
                    try:
                        self.exercise(action)
                    finally:
                        self.fd, self.root = original_fd, original_root

    def test_failed_or_uncertain_grant_is_also_revoked(self):
        with self.assertRaisesRegex(launcher.Refused, "grant-failed"):
            self.exercise("prepare", "grant")

    def test_operation_failure_is_revoked(self):
        with self.assertRaisesRegex(launcher.Refused, "operation-failed"):
            self.exercise("prepare", "operation")

    def test_invalid_receipt_is_not_published_and_grant_is_revoked(self):
        with self.assertRaises(launcher.Refused):
            self.exercise("prepare", "receipt")

    def test_interrupt_still_revokes(self):
        with self.assertRaises(KeyboardInterrupt):
            self.exercise("prepare", "interrupt")

    def test_revocation_failure_never_reports_success(self):
        with self.assertRaisesRegex(launcher.Refused, "revocation-failed"):
            self.exercise("prepare", "revoke")

    def test_json_is_only_data_and_duplicate_or_oversized_input_is_refused(self):
        for raw in (b'project_id=$(touch /outside)', b'{"x":1,"x":2}', b'{"x":NaN}',
                    b'[]', b' ' * (launcher.MAX_BYTES + 1)):
            with self.assertRaises(launcher.Refused):
                launcher.parse_json(raw)
        marker = "$(touch /outside); source /private/state.env"
        self.assertEqual(launcher.parse_json(json.dumps({"text": marker}).encode())["text"], marker)

    def test_subject_and_operation_conflicts_are_refused_before_grant(self):
        for action in ("attest", "install"):
            with self.assertRaisesRegex(launcher.Refused, "separate-subject"):
                launcher.request_scope(action, request(action), OPERATOR, OPERATOR)
        value = request("attest")
        value["verifier_subject_sha256"] = launcher.subject_digest(OPERATOR.uid)
        with self.assertRaisesRegex(launcher.Refused, "verifier-identity"):
            launcher.request_scope("attest", value, OPERATOR, CI)
        value = request("prepare")
        value["project_id"] = "/external/plans"
        with self.assertRaisesRegex(launcher.Refused, "exact-scope"):
            launcher.request_scope("prepare", value, OPERATOR, OPERATOR)
        with self.assertRaisesRegex(launcher.Refused, "wrong-contract"):
            launcher.request_scope("install", request("prepare"), OPERATOR, CI)

    def test_child_command_has_clean_environment_exact_fds_and_drops_authority(self):
        with mock.patch.dict(os.environ, {"ANTDELIRIUM_PLAN_PATH": "/outside/plans", "LD_PRELOAD": "/untrusted"}):
            with mock.patch.object(launcher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
                launcher.InstalledClient().invoke(CI, ["operator", "task-package", "attest"],
                                                  {"ADMIN_CAPABILITY_FD": 8, "TASK_PACKAGE_INPUT_FD": 9,
                                                   "TASK_PACKAGE_OUTPUT_FD": 10})
                args, options = run.call_args
                self.assertIn("--no-new-privs", args[0])
                self.assertIn("--bounding-set=-all", args[0])
                self.assertIn(f"--reuid={CI.uid}", args[0])
                self.assertEqual(options["pass_fds"], (8, 9, 10))
                self.assertEqual(options["env"]["ANTDELIRIUM_ADMIN_SOCKET"], launcher.SOCKET)
                self.assertNotIn("LD_PRELOAD", options["env"])
                self.assertNotIn("ANTDELIRIUM_PLAN_PATH", options["env"])
                self.assertEqual(options["stderr"], subprocess.DEVNULL)
                self.assertFalse(options.get("shell", False))

    def test_readonly_input_is_sealed_and_cannot_be_rewritten(self):
        with launcher.sealed_input(b"private bytes") as descriptor:
            self.assertEqual(os.read(descriptor, 100), b"private bytes")
            with self.assertRaises(OSError):
                os.write(descriptor, b"replacement")
            writer = os.open(f"/proc/self/fd/{descriptor}", os.O_RDWR)
            try:
                with self.assertRaises(OSError):
                    os.write(writer, b"replacement")
            finally:
                os.close(writer)

    def test_output_cannot_overwrite_or_follow_a_symlink(self):
        sentinel = self.root / "existing.json"
        sentinel.write_bytes(b"preserve")
        (self.root / "alias.json").symlink_to(sentinel)
        for name in ("existing.json", "alias.json"):
            with self.assertRaises(FileExistsError):
                launcher.private_output(self.fd, name, OPERATOR)
        self.assertEqual(sentinel.read_bytes(), b"preserve")

    def test_existing_group_writable_target_is_accepted_without_chmod(self):
        (self.root / "target").mkdir(mode=0o775)
        (self.root / "target").chmod(0o775)
        with mock.patch.object(launcher, "REPOSITORY", self.root):
            run_id, run_fd, private_fd = launcher.new_log_run(OPERATOR)
        try:
            self.assertEqual(stat.S_IMODE((self.root / "target").stat().st_mode), 0o775)
            self.assertEqual(stat.S_IMODE(os.fstat(run_fd).st_mode), 0o755)
            self.assertEqual(stat.S_IMODE(os.fstat(private_fd).st_mode), 0o700)
            self.assertNotIn("/", run_id)
        finally:
            os.close(private_fd)
            os.close(run_fd)

    def test_log_parent_symlink_is_refused_without_following_it(self):
        outside = self.root / "sentinel"
        outside.mkdir()
        (self.root / "target").symlink_to(outside)
        with mock.patch.object(launcher, "REPOSITORY", self.root):
            with self.assertRaises(OSError):
                launcher.new_log_run(OPERATOR)
        self.assertEqual(list(outside.iterdir()), [])

    def test_ci_account_needs_access_to_the_admin_socket_without_changing_groups(self):
        directory = SimpleNamespace(st_mode=stat.S_IFDIR | 0o750, st_uid=0, st_gid=123)
        endpoint = SimpleNamespace(st_mode=stat.S_IFSOCK | 0o660, st_uid=0, st_gid=123)
        with mock.patch.object(launcher.os, "lstat", side_effect=[directory, endpoint]), \
             mock.patch.object(launcher.pwd, "getpwuid", return_value=SimpleNamespace(pw_name="fixture")), \
             mock.patch.object(launcher.os, "getgrouplist", return_value=[123]):
            launcher.socket_subjects(OPERATOR, CI)
        with mock.patch.object(launcher.os, "lstat", side_effect=[directory, endpoint]), \
             mock.patch.object(launcher.pwd, "getpwuid", return_value=SimpleNamespace(pw_name="fixture")), \
             mock.patch.object(launcher.os, "getgrouplist", return_value=[456]):
            with self.assertRaisesRegex(launcher.Refused, "admin-group-membership"):
                launcher.socket_subjects(OPERATOR, CI)

    def test_malformed_grant_is_unconfirmed_without_leaking_partial_credentials(self):
        class Malformed(FakeClient):
            def invoke(inner, subject, args, bindings):
                os.write(bindings["CAPABILITY_OUTPUT_FD"], b'{"token":"private-partial')
                return 124
        summary = {"revocation": "not-issued"}
        with mock.patch.object(launcher.os, "fchown"):
            with self.assertRaises(launcher.Refused):
                launcher.execute("prepare", json.dumps(request("prepare")).encode(), OPERATOR, OPERATOR,
                                 self.fd, MANAGER, Malformed(self, "prepare", request("prepare")), summary)
        self.assertEqual(summary["revocation"], "unconfirmed")
        self.assertNotIn("private-partial", json.dumps(summary))
        self.assertEqual((self.root / "grant.json").read_bytes(), b"")

    def test_private_stdio_requires_real_files_and_an_empty_output(self):
        source = self.root / "source.json"
        source.write_bytes(b'{"contract_version":"fixture"}')
        source.chmod(0o600)
        destination = self.root / "out.json"
        destination.touch(mode=0o600)
        source_fd = os.open(source, os.O_RDONLY)
        output_fd = os.open(destination, os.O_WRONLY)
        fstat, fcntl_call, lseek, read = os.fstat, fcntl.fcntl, os.lseek, os.read
        mapped = {0: source_fd, 1: output_fd}
        try:
            with mock.patch.object(launcher.os, "fstat", side_effect=lambda fd: fstat(mapped[fd])), \
                 mock.patch.object(launcher.fcntl, "fcntl", side_effect=lambda fd, op: fcntl_call(mapped[fd], op)), \
                 mock.patch.object(launcher.os, "lseek", side_effect=lambda fd, *args: lseek(mapped[fd], *args)), \
                 mock.patch.object(launcher.os, "read", side_effect=lambda fd, size: read(mapped[fd], size)):
                self.assertEqual(launcher.checked_stdio(OPERATOR), source.read_bytes())
                lseek(source_fd, 0, os.SEEK_SET)
                os.write(output_fd, b"existing")
                lseek(output_fd, 0, os.SEEK_SET)
                with self.assertRaisesRegex(launcher.Refused, "output-descriptor"):
                    launcher.checked_stdio(OPERATOR)
        finally:
            os.close(source_fd)
            os.close(output_fd)

    def test_main_records_only_sanitized_failure_and_never_emits_a_failed_receipt(self):
        private = self.root / "private"
        private.mkdir(mode=0o700)
        private_fd = os.open(private, os.O_RDONLY | os.O_DIRECTORY)
        summary_fd = os.dup(self.fd)
        data = json.dumps(request("prepare")).encode()
        def operation(*args):
            args[-1].update(revocation="failed", capability_id=TOKEN["capability"]["capability_id"])
            raise launcher.Refused("revocation-failed")
        stderr = io.StringIO()
        with mock.patch.object(launcher, "identities", return_value=(OPERATOR, OPERATOR)), \
             mock.patch.object(launcher, "checked_stdio", return_value=data), \
             mock.patch.object(launcher, "trusted_file"), \
             mock.patch.object(launcher, "socket_subjects"), \
             mock.patch.object(launcher, "new_log_run", return_value=("fixture-run", summary_fd, private_fd)), \
             mock.patch.object(launcher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, MANAGER)), \
             mock.patch.object(launcher, "execute", side_effect=operation), \
             mock.patch.object(launcher, "write_all") as write, mock.patch.object(launcher.sys, "stderr", stderr):
            self.assertEqual(launcher.main(["prepare"]), 1)
            write.assert_not_called()
        summary = (self.root / "result.json").read_text()
        self.assertEqual(json.loads((self.root / "started.json").read_text())["revocation"], "unconfirmed")
        self.assertEqual(json.loads(summary)["revocation"], "failed")
        self.assertNotIn("B" * 43, summary + stderr.getvalue())
        self.assertEqual(stat.S_IMODE((self.root / "result.json").stat().st_mode), 0o644)


class context_fd:
    def __init__(self, folder):
        self.fd = os.open(folder, os.O_RDONLY | os.O_DIRECTORY)
    def __enter__(self):
        return self.fd
    def __exit__(self, *args):
        os.close(self.fd)


if __name__ == "__main__":
    unittest.main()
