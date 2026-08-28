#!/usr/bin/env python3
import os
import socket
import struct
import sys


def receive_exact(connection: socket.socket, length: int) -> bytes:
    payload = bytearray()
    while len(payload) < length:
        block = connection.recv(length - len(payload))
        if not block:
            raise RuntimeError("client closed the handshake")
        payload.extend(block)
    return bytes(payload)


def main() -> int:
    if sys.argv[1:] == ["--check"]:
        return 0
    if len(sys.argv) != 3:
        return 64

    socket_path, report_path = sys.argv[1:]
    if not os.path.isabs(socket_path) or os.path.lexists(socket_path):
        return 78

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.settimeout(10)
    server.bind(socket_path)
    server.listen(1)
    with open(report_path + ".ready", "x", encoding="ascii") as ready:
        ready.write("ready\n")

    connection, _ = server.accept()
    with connection:
        connection.settimeout(5)
        if receive_exact(connection, 4) != b"ADL1":
            raise RuntimeError("invalid client frame magic")
        frame_length = struct.unpack(">I", receive_exact(connection, 4))[0]
        if frame_length == 0 or frame_length > 4 * 1024 * 1024:
            raise RuntimeError("invalid client frame length")
        receive_exact(connection, frame_length)
        malformed = b"{}"
        connection.sendall(b"ADL1" + struct.pack(">I", len(malformed)) + malformed)

    server.close()
    with open(report_path, "x", encoding="ascii") as report:
        report.write("accepted=1\nmalformed_response=1\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
