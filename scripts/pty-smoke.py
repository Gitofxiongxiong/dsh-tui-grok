#!/usr/bin/env python3
"""Small PTY smoke for the Grok-derived successor UI.

It checks the new vertical slice: load a real mock session, open/close the
Grok picker, and leave the terminal in a restored state. More specialized
view behavior belongs in widget tests and future golden PTY fixtures.
"""

from __future__ import annotations

import argparse
import errno
import fcntl
import os
import pty
import re
import select
import signal
import struct
import termios
import time
from pathlib import Path


CURSOR_QUERY = b"\x1b[6n"
CURSOR_REPLY = b"\x1b[1;1R"
ANSI_RE = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")


def resize(fd: int, rows: int, cols: int) -> None:
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--mock",
        type=Path,
        default=Path(__file__).parents[1] / "crates/dsh-pager-bin/tests/mock-server.mjs",
    )
    parser.add_argument("--timeout", type=float, default=12.0)
    args = parser.parse_args()

    pid, fd = pty.fork()
    if pid == 0:
        os.execv(
            str(args.binary),
            [
                str(args.binary),
                "--backend",
                "node",
                "--backend-arg",
                str(args.mock),
            ],
        )
    resize(fd, 30, 100)
    output = bytearray()
    deadline = time.monotonic() + args.timeout

    def pump(seconds: float = 0.1) -> None:
        readable, _, _ = select.select([fd], [], [], seconds)
        if not readable:
            return
        try:
            chunk = os.read(fd, 16384)
        except OSError as error:
            if error.errno in (errno.EIO, errno.EBADF):
                return
            raise
        output.extend(chunk)
        if CURSOR_QUERY in chunk:
            os.write(fd, CURSOR_REPLY)

    def visible() -> bytes:
        return ANSI_RE.sub(b"", bytes(output))

    try:
        while b"SessionLoaded" not in visible() and time.monotonic() < deadline:
            pump()
        if b"SessionLoaded" not in visible():
            raise RuntimeError("timed out loading mock session")

        # Grok picker vertical slice: open, exercise search input, close.
        os.write(fd, b"p")
        while b"search:" not in visible() and time.monotonic() < deadline:
            pump()
        if b"search:" not in visible():
            raise RuntimeError("picker did not render its search bar")
        # Leave the query empty so the first Esc exercises Grok's close path
        # (a non-empty query intentionally consumes Esc to clear itself).
        pump(0.2)
        before_close = len(output)
        os.write(fd, b"\x1b")
        # The inline terminal paints only changed cells, so the status line
        # may arrive as a short diff rather than the complete phrase. Wait
        # for the first changed footer cell before sending the shell Esc.
        while time.monotonic() < deadline:
            pump()
            if b"clos" in visible()[before_close:]:
                break

        # Exit through the same Esc path used by the interactive shell.
        os.write(fd, b"\x1b")
        while time.monotonic() < deadline:
            pump()
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited == pid:
                code = os.waitstatus_to_exitcode(status)
                if code != 0:
                    raise RuntimeError(f"dsh-pager exited with {code}")
                if b"\x1b[?1049l" not in output or b"\x1b[?25h" not in output:
                    raise RuntimeError("terminal surface was not restored")
                return 0
        os.kill(pid, signal.SIGKILL)
        try:
            os.waitpid(pid, 0)
        except ChildProcessError:
            pass
        raise RuntimeError("dsh-pager did not exit before the PTY timeout")
    except Exception:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            os.waitpid(pid, 0)
        except ChildProcessError:
            pass
        raise


if __name__ == "__main__":
    raise SystemExit(main())
