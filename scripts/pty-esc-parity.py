#!/usr/bin/env python3
"""PTY contract for Grok's clear/rewind/cancel Esc ladder."""

from __future__ import annotations

import argparse
import errno
import importlib.util
import os
import pty
import select
import signal
import time
from pathlib import Path


ROOT = Path(__file__).parents[1]
SMOKE_PATH = ROOT / "scripts" / "pty-smoke.py"
SPEC = importlib.util.spec_from_file_location("dsh_pty_smoke", SMOKE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import {SMOKE_PATH}")
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--mock",
        type=Path,
        default=ROOT / "crates" / "dsh-pager-bin" / "tests" / "mock-server.mjs",
    )
    parser.add_argument("--timeout", type=float, default=35.0)
    args = parser.parse_args()

    pid, fd = pty.fork()
    if pid == 0:
        os.environ["GROK_ESC_DOUBLE_PRESS_MS"] = "5000"
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

    SMOKE.resize(fd, 30, 100)
    screen = SMOKE.AnsiScreen(30, 100)
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
        screen.feed(chunk)
        if SMOKE.CURSOR_QUERY in chunk:
            os.write(fd, SMOKE.CURSOR_REPLY)

    def wait_for(predicate, message: str) -> None:
        while time.monotonic() < deadline:
            pump()
            if predicate():
                return
        raise RuntimeError(f"{message}; screen tail={screen.text()[-900:]!r}")

    def raw_text() -> str:
        return SMOKE.ANSI_RE.sub(b"", bytes(output)).decode("utf-8", "replace")

    try:
        wait_for(lambda: "SessionLoaded" in raw_text(), "session did not load")

        os.write(fd, b"xraft-to-clear")
        wait_for(lambda: "xraft-to-clear" in screen.text(), "draft was not rendered")
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "press again to clear" in screen.text(),
            "first Esc did not arm visible clear confirmation",
        )
        if "Rewind to which turn?" in screen.text():
            raise RuntimeError("draft Esc incorrectly opened rewind")
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "xraft-to-clear" not in screen.text()
            and "press again to clear" not in screen.text(),
            "second Esc did not clear the draft",
        )

        os.write(fd, b"\x1b")
        pump(0.15)
        if "Rewind to which turn?" in screen.text():
            raise RuntimeError("first empty-prompt Esc must silently arm rewind")
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "Rewind to which turn?" in screen.text(),
            "second empty-prompt Esc did not open rewind picker",
        )
        os.write(fd, b"\r")
        wait_for(
            lambda: "Rewind conversation to" in screen.text(),
            "rewind picker Enter did not open Grok confirmation",
        )
        os.write(fd, b"y")
        wait_for(
            lambda: "Conversation rewound" in screen.text()
            and "hello from history" in screen.text(),
            "rewind did not attach an empty session and restore the selected prompt",
        )
        if "history is loaded" in screen.text():
            raise RuntimeError("rewind retained the selected turn's assistant response")
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "press again to clear" in screen.text(),
            "restored rewind prompt did not arm clear",
        )
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "hello from history" not in screen.text(),
            "restored rewind prompt did not follow the double-Esc clear policy",
        )

        os.write(fd, b"cancel smoke\r")
        wait_for(
            lambda: "Esc:cancel" in screen.text(),
            "cancel fixture did not reach the running snapshot",
        )
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "Cancellation accepted" in screen.text()
            and "Esc:cancel" in screen.text(),
            "first cancel was not accepted while the host stayed running",
        )
        if "Turn cancelled" in screen.text():
            raise RuntimeError("first cancel unexpectedly converged; retry fixture is invalid")
        os.write(fd, b"\x1b")
        wait_for(
            lambda: "Turn cancelled" in screen.text(),
            "cancel-pending Esc did not resend and converge through host snapshot",
        )

        # Grok's idle Esc never quits. Ctrl+C is the empty-idle exit rung.
        os.write(fd, b"\x03")
        while time.monotonic() < deadline:
            pump()
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited != pid:
                continue
            code = os.waitstatus_to_exitcode(status)
            if code != 0:
                raise RuntimeError(f"dsh-pager exited with {code}")
            if b"\x1b[?1049l" not in output or b"\x1b[?25h" not in output:
                raise RuntimeError("terminal surface was not restored")
            return 0
        raise RuntimeError("dsh-pager did not exit after idle Ctrl+C")
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
