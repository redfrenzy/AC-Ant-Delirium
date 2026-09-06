#!/usr/bin/env python3
"""Compute the rust-project-source-v1 fingerprint used by the protected backup."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import stat
import sys


EXCLUDED_ROOT = {".git", "target", "guard", "plugins", "AC_Ant-Delirium_props"}
MAX_DEPTH = 64
MAX_ENTRIES = 100_000
MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_TOTAL_BYTES = 2 * 1024 * 1024 * 1024
MAX_PATH_BYTES = 4096


class FingerprintFailure(Exception):
    pass


def collect(root: Path) -> list[tuple[str, bool, int, bytes]]:
    entries: list[tuple[str, bool, int, bytes]] = []
    total = 0

    def visit(directory: Path, prefix: str, depth: int) -> None:
        nonlocal total
        if depth > MAX_DEPTH:
            raise FingerprintFailure("source depth limit exceeded")
        try:
            children = list(os.scandir(directory))
        except OSError as error:
            raise FingerprintFailure("source directory is unavailable") from error
        for child in children:
            name = child.name
            if name == ".git" or (not prefix and name in EXCLUDED_ROOT):
                continue
            if not name or "/" in name or "\\" in name:
                raise FingerprintFailure("invalid source name")
            relative = name if not prefix else f"{prefix}/{name}"
            relative_bytes = relative.encode("utf-8")
            if len(relative_bytes) > MAX_PATH_BYTES:
                raise FingerprintFailure("source path limit exceeded")
            try:
                metadata = child.stat(follow_symlinks=False)
            except OSError as error:
                raise FingerprintFailure("source metadata is unavailable") from error
            if stat.S_ISDIR(metadata.st_mode):
                visit(Path(child.path), relative, depth + 1)
                continue
            if not stat.S_ISREG(metadata.st_mode):
                raise FingerprintFailure("source contains a non-regular entry")
            if metadata.st_size < 0 or metadata.st_size > MAX_FILE_BYTES:
                raise FingerprintFailure("source file limit exceeded")
            flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
            try:
                descriptor = os.open(child.path, flags)
                with os.fdopen(descriptor, "rb", closefd=True) as handle:
                    opened = os.fstat(handle.fileno())
                    payload = handle.read(MAX_FILE_BYTES + 1)
            except OSError as error:
                raise FingerprintFailure("source file is unavailable") from error
            if (
                not stat.S_ISREG(opened.st_mode)
                or opened.st_dev != metadata.st_dev
                or opened.st_ino != metadata.st_ino
                or opened.st_size != metadata.st_size
                or len(payload) != metadata.st_size
            ):
                raise FingerprintFailure("source changed during capture")
            total += len(payload)
            if total > MAX_TOTAL_BYTES or len(entries) >= MAX_ENTRIES:
                raise FingerprintFailure("source snapshot limit exceeded")
            entries.append(
                (relative, bool(metadata.st_mode & 0o111), len(payload), hashlib.sha256(payload).digest())
            )

    visit(root, "", 0)
    if not entries:
        raise FingerprintFailure("source snapshot is empty")
    return entries


def fingerprint(root: Path) -> str:
    try:
        metadata = root.lstat()
    except OSError as error:
        raise FingerprintFailure("source root is unavailable") from error
    if root.is_symlink() or not stat.S_ISDIR(metadata.st_mode):
        raise FingerprintFailure("source root is unsafe")
    digest = hashlib.sha256()
    digest.update(b"antdelirium-protected-source-snapshot-v1\0")
    for relative, executable, size, contents_sha256 in sorted(collect(root)):
        encoded = relative.encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
        digest.update(b"\x01" if executable else b"\x00")
        digest.update(size.to_bytes(8, "big"))
        digest.update(contents_sha256)
    return digest.hexdigest()


def main() -> int:
    if len(sys.argv) != 2:
        print("protected-source-fingerprint: expected one absolute source root", file=sys.stderr)
        return 2
    root = Path(sys.argv[1])
    if not root.is_absolute():
        print("protected-source-fingerprint: source root must be absolute", file=sys.stderr)
        return 2
    try:
        print(fingerprint(root))
        return 0
    except (FingerprintFailure, UnicodeError):
        print("protected-source-fingerprint: source snapshot rejected", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
