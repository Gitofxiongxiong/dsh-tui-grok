#!/usr/bin/env python3
"""Small PTY smoke for the Grok-derived successor UI.

It checks the new vertical slice: load a real mock session, run `/resume`,
open/close the Grok session picker, and leave the terminal in a restored
state. More specialized view behavior belongs in widget tests and future
golden PTY fixtures.
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
    parser.add_argument(
        "--backend",
        default="node",
        help="backend executable; defaults to node for the mock Harness",
    )
    parser.add_argument(
        "--backend-arg",
        action="append",
        default=[],
        help="backend argument; may be repeated",
    )
    parser.add_argument(
        "--pager-arg",
        action="append",
        default=[],
        help="dsh-pager argument; may be repeated",
    )
    parser.add_argument("--timeout", type=float, default=12.0)
    parser.add_argument(
        "--full",
        action="store_true",
        help="exercise resize, queue, mouse and terminal restore before exit",
    )
    args = parser.parse_args()

    pid, fd = pty.fork()
    if pid == 0:
        backend_args = args.backend_arg or [str(args.mock)]
        command = [str(args.binary), *args.pager_arg, "--backend", args.backend]
        for value in backend_args:
            command.extend(["--backend-arg", value])
        os.execv(
            str(args.binary),
            command,
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

    def visible_since(raw_offset: int) -> bytes:
        return ANSI_RE.sub(b"", bytes(output[raw_offset:]))

    try:
        while b"SessionLoaded" not in visible() and time.monotonic() < deadline:
            pump()
        if b"SessionLoaded" not in visible():
            raise RuntimeError("timed out loading backend session")

        # Grok native resume vertical slice: dispatch the local slash command,
        # wait for modal chrome and its inactive search hint, activate search,
        # then close it with an empty query.
        before_resume = len(output)
        os.write(fd, b"/resume\r")
        while time.monotonic() < deadline:
            pump()
            resume_output = visible_since(before_resume)
            if b"Resume session" in resume_output and b"/ to search" in resume_output:
                break
        resume_output = visible_since(before_resume)
        if b"Resume session" not in resume_output:
            raise RuntimeError("/resume did not render the native session modal")
        if b"/ to search" not in resume_output:
            raise RuntimeError("resume picker did not render its search hint")
        before_search = len(output)
        os.write(fd, b"/")
        while b"search:" not in visible_since(before_search) and time.monotonic() < deadline:
            pump()
        if b"search:" not in visible_since(before_search):
            raise RuntimeError("resume picker did not activate its search bar")
        # Grok's first Esc leaves the active search field; the second closes
        # the modal. The query stays empty throughout this focus ladder.
        pump(0.2)
        before_close = len(output)
        os.write(fd, b"\x1b")
        pump(0.2)
        os.write(fd, b"\x1b")
        # The inline terminal paints only changed cells, so the status line
        # may arrive as a short diff rather than the complete phrase. Wait
        # for the first changed footer cell before sending the shell Esc.
        while time.monotonic() < deadline:
            pump()
            if b"clos" in visible_since(before_close):
                break

        # The removed shortcut must stay removed: plain `p` edits the prompt
        # and cannot reopen the resume modal. Backspace restores an empty draft
        # before the existing queue/full and final Esc paths continue.
        before_plain_p = len(output)
        os.write(fd, b"p")
        pump(0.3)
        if b"Resume session" in visible_since(before_plain_p):
            raise RuntimeError("plain p unexpectedly reopened the resume picker")
        os.write(fd, b"\x7f")
        pump(0.1)

        if args.full:
            # M8/M10 input matrix smoke: resize invalidates geometry, queue and
            # mouse overlays route through their owners before terminal restore.
            resize(fd, 40, 80)
            os.write(fd, b"q")
            while b"Queue" not in visible() and time.monotonic() < deadline:
                pump()
            if b"Queue" not in visible():
                raise RuntimeError("queue overlay did not render")
            os.write(fd, b"\x1b")
            pump(0.2)
            # A SGR mouse wheel event and resize storm must be accepted even
            # when they do not change the selected row; the assertion is
            # terminal liveness. Prompt/interaction RPC paths are covered by
            # the binary mock Harness tests in scripts/e2e.sh.
            os.write(fd, b"\x1b[<64;5;5M")
            resize(fd, 30, 100)
            pump(0.6)

        # Exit through the same Esc path used by the interactive shell.
        os.write(fd, b"\x1b")
        if args.full:
            # Keep the close and quit steps separate so an implementation with
            # an extra modal layer still follows the Esc ladder deterministically.
            pump(0.2)
            os.write(fd, b"\x1b\x1b")
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
        raise RuntimeError(f"dsh-pager did not exit before the PTY timeout; tail={visible()[-500:]!r}")
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
