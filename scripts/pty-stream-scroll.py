#!/usr/bin/env python3
"""PTY regression for Grok-compatible scrolling during a live token stream."""

from __future__ import annotations

import argparse
import errno
import importlib.util
import os
import pty
import re
import select
import signal
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SMOKE_PATH = ROOT / "scripts" / "pty-smoke.py"
SPEC = importlib.util.spec_from_file_location("dsh_pty_smoke", SMOKE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SMOKE_PATH}")
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)

AnsiScreen = SMOKE.AnsiScreen
CURSOR_QUERY = SMOKE.CURSOR_QUERY
CURSOR_REPLY = SMOKE.CURSOR_REPLY
resize = SMOKE.resize

MARKER_RE = re.compile(r"SCROLL-MARKER-(\d{4})")
TAIL_RE = re.compile(r"TAIL-LIVE-(\d{4})")
ROWS = 40
COLS = 100
WHEEL_ROW = 15
WHEEL_COL = 50


def wheel(button: int, count: int = 1) -> bytes:
    report = f"\x1b[<{button};{WHEEL_COL};{WHEEL_ROW}M".encode()
    return report * count


def visible_marker_rows(screen: AnsiScreen) -> list[tuple[int, int]]:
    rows: list[tuple[int, int]] = []
    for row, line in enumerate(screen.text().splitlines()):
        match = MARKER_RE.search(line)
        if match:
            rows.append((int(match.group(1)), row))
    return rows


def visible_tail_indices(screen: AnsiScreen) -> list[int]:
    return [int(value) for value in TAIL_RE.findall(screen.text())]


def common_marker_row_delta(
    before: list[tuple[int, int]],
    after: list[tuple[int, int]],
) -> int | None:
    before_rows = dict(before)
    after_rows = dict(after)
    deltas = {
        after_rows[marker] - before_row
        for marker, before_row in before_rows.items()
        if marker in after_rows
    }
    return deltas.pop() if len(deltas) == 1 else None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--mock",
        type=Path,
        default=ROOT / "crates" / "dsh-pager-bin" / "tests" / "mock-server.mjs",
    )
    parser.add_argument("--timeout", type=float, default=18.0)
    args = parser.parse_args()

    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.environ["TERM_PROGRAM"] = "zed"
        os.environ["GROK_SCROLL_MODE"] = "wheel"
        os.environ["GROK_SCROLL_LINES"] = "1"
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

    resize(fd, ROWS, COLS)
    screen = AnsiScreen(ROWS, COLS)
    output = bytearray()
    deadline = time.monotonic() + args.timeout

    def pump(seconds: float = 0.05) -> None:
        readable, _, _ = select.select([fd], [], [], seconds)
        if not readable:
            return
        try:
            chunk = os.read(fd, 65536)
        except OSError as error:
            if error.errno in (errno.EIO, errno.EBADF):
                return
            raise
        output.extend(chunk)
        screen.feed(chunk)
        if CURSOR_QUERY in chunk:
            os.write(fd, CURSOR_REPLY)

    def wait_until(predicate, label: str) -> None:
        while time.monotonic() < deadline:
            pump()
            if predicate():
                return
        raise RuntimeError(f"timed out waiting for {label}\nscreen:\n{screen.text()}")

    def wait_for_exit() -> None:
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
            return
        raise RuntimeError("dsh-pager did not exit before timeout")

    try:
        wait_until(lambda: "SessionLoaded" in screen.text(), "loaded session")
        wait_until(
            lambda: "Space:prompt" in screen.text() or "Enter:send" in screen.text(),
            "interactive surface readiness",
        )
        if "Space:prompt" in screen.text():
            os.write(fd, b" ")
            wait_until(
                lambda: "Space:prompt" not in screen.text()
                and "Ctrl+o:yolo" in screen.text(),
                "interactive prompt focus",
            )
        # The focus helper is a real Space key. Clear any character admitted
        # at the owner transition before entering the exact mock fixture.
        os.write(fd, b"\x15")
        pump(0.05)
        os.write(fd, b"stream scroll smoke")
        wait_until(
            lambda: "stream scroll smoke" in screen.text()
            and "Enter:send" in screen.text(),
            "stable prompt draft",
        )
        os.write(fd, b"\r")
        wait_until(
            lambda: bool(visible_marker_rows(screen)) and bool(visible_tail_indices(screen)),
            "streaming marker fixture",
        )
        time.sleep(0.15)
        pump(0.05)

        before_rows = visible_marker_rows(screen)
        if not before_rows:
            raise RuntimeError(f"no baseline marker\nscreen:\n{screen.text()}")
        top_before, top_before_row = before_rows[0]

        # Grok forced-wheel pricing on a Zed ept=1 profile: one report is one row.
        os.write(fd, wheel(64))
        wait_until(
            lambda: common_marker_row_delta(before_rows, visible_marker_rows(screen)) == 1,
            "one-row wheel-up movement",
        )
        parked_rows = visible_marker_rows(screen)
        row_delta = common_marker_row_delta(before_rows, parked_rows)
        if row_delta != 1:
            raise AssertionError(
                "forced wheel report must move exactly one row: "
                f"delta={row_delta}\nscreen:\n{screen.text()}"
            )
        parked_marker = top_before
        parked_screen_row = dict(parked_rows)[parked_marker]

        # No more input: live deltas continue, but the parked marker must not drift.
        observe_until = time.monotonic() + 0.7
        while time.monotonic() < observe_until:
            pump()
        later_rows = visible_marker_rows(screen)
        later_parked_row = dict(later_rows).get(parked_marker)
        if later_parked_row != parked_screen_row:
            raise AssertionError(
                "parked marker moved while live deltas arrived: "
                f"{(parked_marker, parked_screen_row)} -> "
                f"{(parked_marker, later_parked_row)}\nscreen:\n{screen.text()}"
            )

        # Travel to the tail with a bounded, human-sized gesture.  The previous
        # regression sent 80 reports as one flood, which could hide a moving or
        # stale max_scroll: brute force eventually crossed either value.  Send
        # one-row reports and require the real tail to become visible within a
        # viewport-sized budget, just like the upstream Grok PTY scenario.
        down_reports = 0
        down_budget = ROWS // 2 + 4
        while down_reports < down_budget:
            os.write(fd, wheel(65))
            down_reports += 1
            pump(0.012)
        if not visible_tail_indices(screen):
            raise AssertionError(
                "bounded wheel-down could not reach the live tail: "
                f"reports={down_reports}\nscreen:\n{screen.text()}"
            )

        # Landing at the bottom remains manual in Grok.  One additional fully
        # clamped downward report is the explicit gesture that restores follow.
        os.write(fd, wheel(65))
        pump(0.05)
        tail_after_down = max(visible_tail_indices(screen))
        follow_until = time.monotonic() + 1.0
        while time.monotonic() < follow_until:
            pump()
        tails_following = visible_tail_indices(screen)
        if not tails_following or max(tails_following) <= tail_after_down:
            raise AssertionError(
                "fully-clamped wheel-down did not restore live follow: "
                f"tail stayed at {tail_after_down}\nscreen:\n{screen.text()}"
            )

        # Ctrl+C follows the Grok cancel-then-quit ladder while the mock stream
        # remains live. The second press exits after the pending cancel receipt.
        os.write(fd, b"\x03")
        time.sleep(0.15)
        pump(0.05)
        os.write(fd, b"\x03")
        wait_for_exit()
        print(
            "stream scroll PTY ok: "
            f"marker={top_before} row={top_before_row}->{parked_screen_row}, "
            f"down_reports={down_reports}, "
            f"tail={tail_after_down}->"
            f"{max(tails_following)}"
        )
        return 0
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
