#!/usr/bin/env python3
"""Wait until the installed administrative protocol completes its real hello."""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import socket
import stat
import struct
import sys


MAGIC = b"ADA1"
WIRE_VERSION = "ada/1"
MAX_FRAME_BYTES = 4 * 1024 * 1024
TIMEOUT_SECONDS = 30.0


def fail(message: str) -> None:
    print(f"service-readiness: {message}", file=sys.stderr)
    raise SystemExit(1)


def receive_exact(connection: socket.socket, size: int) -> bytes:
    output = bytearray()
    while len(output) < size:
        chunk = connection.recv(size - len(output))
        if not chunk:
            fail("administrative protocol closed before readiness")
        output.extend(chunk)
    return bytes(output)


def main() -> None:
    if len(sys.argv) != 2:
        fail("one absolute administrative socket is required")
    endpoint = Path(sys.argv[1])
    if not endpoint.is_absolute():
        fail("administrative endpoint must be absolute")
    try:
        metadata = endpoint.lstat()
    except OSError:
        fail("administrative endpoint is unavailable")
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISSOCK(metadata.st_mode):
        fail("administrative endpoint metadata is unsafe")

    nonce = base64.urlsafe_b64encode(os.urandom(32)).decode("ascii").rstrip("=")
    hello = json.dumps(
        {
            "message_type": "client_hello",
            "channel": "admin",
            "wire_versions": [WIRE_VERSION],
            "client_nonce": nonce,
        },
        ensure_ascii=True,
        allow_nan=False,
        separators=(",", ":"),
    ).encode("ascii")
    frame = MAGIC + struct.pack(">I", len(hello)) + hello

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(TIMEOUT_SECONDS)
            connection.connect(str(endpoint))
            connection.sendall(frame)
            header = receive_exact(connection, 8)
            if header[:4] != MAGIC:
                fail("administrative protocol returned invalid framing")
            size = struct.unpack(">I", header[4:])[0]
            if size == 0 or size > MAX_FRAME_BYTES:
                fail("administrative protocol returned an invalid frame size")
            response = json.loads(receive_exact(connection, size))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"administrative protocol did not become ready: {type(error).__name__}")

    if not isinstance(response, dict) or set(response) != {
        "message_type",
        "wire_version",
        "connection_nonce",
        "max_frame_bytes",
    }:
        fail("administrative readiness response has an invalid contract")
    connection_nonce = response["connection_nonce"]
    if (
        response["message_type"] != "server_hello"
        or response["wire_version"] != WIRE_VERSION
        or response["max_frame_bytes"] != MAX_FRAME_BYTES
        or not isinstance(connection_nonce, str)
        or len(connection_nonce) != 43
    ):
        fail("administrative readiness response is invalid")
    try:
        decoded = base64.urlsafe_b64decode(connection_nonce + "=")
    except (ValueError, base64.binascii.Error):
        fail("administrative readiness nonce is invalid")
    if len(decoded) != 32:
        fail("administrative readiness nonce is invalid")


if __name__ == "__main__":
    main()
