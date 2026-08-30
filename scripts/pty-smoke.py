#!/usr/bin/env python3
"""Small PTY smoke for the Grok-derived successor UI.

It checks the vertical slice: load a real mock session, save a masked fake
DeepSeek credential through `/login`, run `/resume`, open/close the Grok
session picker, and leave the terminal in a restored state. More specialized
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
import unicodedata
from pathlib import Path


CURSOR_QUERY = b"\x1b[6n"
CURSOR_REPLY = b"\x1b[1;1R"
ANSI_RE = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")
CSI_RE = re.compile(rb"\x1b\[([0-9;?]*)([@-~])")


class AnsiScreen:
    """Small screen model for assertions against ratatui's delta paints.

    The UI intentionally paints only changed cells. Removing ANSI controls is
    therefore not enough: a title such as ``Resume session`` is emitted as
    individually positioned characters and can look scrambled in a byte
    stream while being perfectly correct on the terminal. This model handles
    the cursor/erase operations used by the TUI without adding a dependency
    on a full terminal emulator.
    """

    def __init__(self, rows: int, cols: int) -> None:
        self.rows = rows
        self.cols = cols
        self.grid = [[" "] * cols for _ in range(rows)]
        self.row = 0
        self.col = 0
        self.saved = (0, 0)

    def resize(self, rows: int, cols: int) -> None:
        old = self.grid
        self.rows = rows
        self.cols = cols
        self.grid = [[" "] * cols for _ in range(rows)]
        for row in range(min(rows, len(old))):
            self.grid[row][: min(cols, len(old[row]))] = old[row][:cols]
        self.row = min(self.row, rows - 1)
        self.col = min(self.col, cols - 1)

    def _clear(self) -> None:
        self.grid = [[" "] * self.cols for _ in range(self.rows)]

    def _put(self, char: str) -> None:
        if char == "\r":
            self.col = 0
            return
        if char == "\n":
            self.row = min(self.rows - 1, self.row + 1)
            return
        if char == "\b":
            self.col = max(0, self.col - 1)
            return
        if ord(char) < 0x20:
            return
        width = 0 if unicodedata.combining(char) else (
            2 if unicodedata.east_asian_width(char) in "WF" else 1
        )
        if width == 0:
            if self.col and self.row < self.rows:
                self.grid[self.row][self.col - 1] += char
            return
        if self.row < self.rows and self.col < self.cols:
            self.grid[self.row][self.col] = char
            if width == 2 and self.col + 1 < self.cols:
                self.grid[self.row][self.col + 1] = " "
        self.col += width
        if self.col >= self.cols:
            self.col = self.cols - 1

    def feed(self, data: bytes) -> None:
        index = 0
        while index < len(data):
            if data[index] != 0x1B:
                try:
                    char = data[index:].decode("utf-8")[0]
                except UnicodeDecodeError:
                    index += 1
                    continue
                self._put(char)
                index += len(char.encode("utf-8"))
                continue
            if data.startswith(b"\x1b]", index):
                bell = data.find(b"\x07", index + 2)
                st = data.find(b"\x1b\\", index + 2)
                ends = [end for end in (bell, st) if end >= 0]
                if not ends:
                    return
                end = min(ends)
                index = end + (1 if end == bell else 2)
                continue
            if data.startswith(b"\x1b[", index):
                match = CSI_RE.match(data, index)
                if match is None:
                    index += 1
                    continue
                raw_params = match.group(1).decode()
                final = match.group(2).decode()
                private = raw_params.startswith("?")
                params = raw_params.lstrip("?").split(";") if raw_params.lstrip("?") else []
                numbers = [int(value) if value else 0 for value in params]

                def amount(position: int, default: int = 1) -> int:
                    return numbers[position] if position < len(numbers) and numbers[position] else default

                if final in "Hf":
                    self.row = max(0, min(self.rows - 1, amount(0) - 1))
                    self.col = max(0, min(self.cols - 1, amount(1) - 1))
                elif final == "A":
                    self.row = max(0, self.row - amount(0))
                elif final == "B":
                    self.row = min(self.rows - 1, self.row + amount(0))
                elif final == "C":
                    self.col = min(self.cols - 1, self.col + amount(0))
                elif final == "D":
                    self.col = max(0, self.col - amount(0))
                elif final == "G":
                    self.col = max(0, min(self.cols - 1, amount(0) - 1))
                elif final == "d":
                    self.row = max(0, min(self.rows - 1, amount(0) - 1))
                elif final == "J" and amount(0, 0) in (2, 3):
                    self._clear()
                elif final == "K":
                    for column in range(self.col, self.cols):
                        self.grid[self.row][column] = " "
                elif final == "s":
                    self.saved = (self.row, self.col)
                elif final == "u":
                    self.row, self.col = self.saved
                elif private and final in "hl" and "1049" in raw_params:
                    self._clear()
                index = match.end()
                continue
            index += 1

    def text(self) -> str:
        return "\n".join("".join(row).rstrip() for row in self.grid)


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
    screen = AnsiScreen(30, 100)
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
        if CURSOR_QUERY in chunk:
            os.write(fd, CURSOR_REPLY)

    def drain_ready() -> None:
        """Consume output already buffered after the child has exited."""
        while select.select([fd], [], [], 0)[0]:
            try:
                chunk = os.read(fd, 16384)
            except OSError as error:
                if error.errno in (errno.EIO, errno.EBADF):
                    return
                raise
            if not chunk:
                return
            output.extend(chunk)
            screen.feed(chunk)

    def visible() -> bytes:
        return ANSI_RE.sub(b"", bytes(output))

    def visible_since(raw_offset: int) -> bytes:
        return ANSI_RE.sub(b"", bytes(output[raw_offset:]))

    try:
        while b"SessionLoaded" not in visible() and time.monotonic() < deadline:
            pump()
        if b"SessionLoaded" not in visible():
            raise RuntimeError("timed out loading backend session")

        # `/login` is local UI over the Host credential seam. The fake value
        # must render only as bullets and must never be echoed into the PTY
        # output, status line, or transcript.
        fake_key = b"sk-pty-placeholder"
        os.write(fd, b"/login\r")
        while time.monotonic() < deadline:
            pump()
            login_screen = screen.text()
            if "Log in to DeepSeek" in login_screen and "Enter your DeepSeek API key" in login_screen:
                break
        login_screen = screen.text()
        if "Log in to DeepSeek" not in login_screen:
            raise RuntimeError("/login did not render the DeepSeek credential modal")
        if "Enter your DeepSeek API key" not in login_screen:
            raise RuntimeError("credential describe did not enable the API key input")
        os.write(fd, fake_key)
        pump(0.3)
        if fake_key.decode() in screen.text() or fake_key in visible():
            raise RuntimeError("DeepSeek API key leaked into terminal output")
        if "•" not in screen.text():
            raise RuntimeError("DeepSeek API key was not rendered as masked input")
        os.write(fd, b"\r")
        while time.monotonic() < deadline:
            pump()
            if "Log in to DeepSeek" not in screen.text():
                break
        if "Log in to DeepSeek" in screen.text():
            raise RuntimeError("credential save did not close the login modal")
        if fake_key in visible():
            raise RuntimeError("DeepSeek API key leaked after credential save")

        # Grok native resume vertical slice: dispatch the local slash command,
        # wait for modal chrome and its inactive search hint, activate search,
        # then close it with an empty query.
        before_resume = len(output)
        os.write(fd, b"/resume\r")
        while time.monotonic() < deadline:
            pump()
            resume_screen = screen.text()
            if "Resume session" in resume_screen and "/ to search" in resume_screen:
                break
        resume_screen = screen.text()
        if "Resume session" not in resume_screen:
            raise RuntimeError("/resume did not render the native session modal")
        if "/ to search" not in resume_screen:
            raise RuntimeError("resume picker did not render its search hint")
        before_search = len(output)
        os.write(fd, b"/")
        while "search:" not in screen.text() and time.monotonic() < deadline:
            pump()
        if "search:" not in screen.text():
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
            if "Resume session" not in screen.text():
                break

        # The removed shortcut must stay removed: plain `p` edits the prompt
        # and cannot reopen the resume modal. Backspace restores an empty draft
        # before the existing queue/full and final Esc paths continue.
        before_plain_p = len(output)
        os.write(fd, b"p")
        pump(0.3)
        if "Resume session" in screen.text():
            raise RuntimeError("plain p unexpectedly reopened the resume picker")
        os.write(fd, b"\x7f")
        pump(0.1)

        if args.full:
            # M8/M10 input matrix smoke: resize invalidates geometry, queue and
            # mouse overlays route through their owners before terminal restore.
            resize(fd, 40, 80)
            screen.resize(40, 80)
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

        # Grok's idle Esc is reserved for clear/rewind and never quits.
        # Exit through the empty-idle Ctrl+C rung instead.
        os.write(fd, b"\x03")
        if args.full:
            # The full matrix can leave an invisible draft after mouse/input
            # probing. The first Ctrl+C clears it; the second exits.
            pump(0.2)
            os.write(fd, b"\x03")
        while time.monotonic() < deadline:
            pump()
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited == pid:
                code = os.waitstatus_to_exitcode(status)
                if code != 0:
                    raise RuntimeError(f"dsh-pager exited with {code}")
                drain_ready()
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
