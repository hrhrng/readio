#!/usr/bin/env python3
"""Drive readio in a real pty and print what the screen would look like.

Usage: pty_probe.py <cols> <rows> <script> [args...]
The script is a comma-separated list of steps: `wait:0.8`, `key:enter`,
`type:进度条`, `key:ctrl-c`.
"""
import os
import pty
import re
import select
import sys
import time

COLS, ROWS = int(sys.argv[1]), int(sys.argv[2])
STEPS = sys.argv[3].split(",")
ARGV = sys.argv[4:]

KEYS = {
    "enter": b"\r",
    "esc": b"\x1b",
    "ctrl-c": b"\x03",
    "ctrl-d": b"\x04",
    "ctrl-l": b"\x0c",
    "ctrl-s": b"\x13",
    "ctrl-t": b"\x14",
    "ctrl-o": b"\x0f",
}

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.environ["COLORTERM"] = "truecolor"
    os.environ["COLUMNS"], os.environ["LINES"] = str(COLS), str(ROWS)
    # crossterm honours NO_COLOR by dropping every SGR colour, so a shell that
    # sets it turns this probe colour-blind. The point here is to see what a
    # normal terminal receives.
    os.environ.pop("NO_COLOR", None)
    os.execv("./target/release/readio", ["readio"] + ARGV)

import fcntl
import struct
import termios

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

captured = bytearray()


def drain(seconds):
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                return
            if not chunk:
                return
            captured.extend(chunk)


def wait_for_raw_mode(timeout=10.0):
    """Block until the app has switched the terminal into raw mode.

    Until then the line discipline still translates CR to NL, so an Enter
    keystroke would arrive as Ctrl+J and be ignored. Entering the alternate
    screen happens in the same setup step, so that sequence is the signal.
    On the first launch after a rebuild macOS can take seconds to exec, which
    is exactly when the race used to bite.
    """
    end = time.time() + timeout
    while time.time() < end:
        if b"\x1b[?1049h" in bytes(captured):
            # Give the raw-mode ioctl the moment it needs to land.
            drain(0.05)
            return True
        drain(0.05)
    raise SystemExit("readio never entered raw mode")


wait_for_raw_mode()


for step in STEPS:
    kind, _, value = step.partition(":")
    if kind == "wait":
        drain(float(value))
    elif kind == "key":
        os.write(fd, KEYS[value])
        drain(0.15)
    elif kind == "type":
        os.write(fd, value.encode())
        drain(0.15)

drain(0.4)
# Snapshot here: the screen as it stands after the scripted steps, before the
# quit keystrokes (which would close overlays).
cut = len(captured)

os.write(fd, KEYS["ctrl-d"])
deadline = time.time() + 3.0
status = None
while time.time() < deadline:
    drain(0.1)
    pid_done, code = os.waitpid(pid, os.WNOHANG)
    if pid_done:
        status = code
        break

raw = bytes(captured[:cut])

# Very small terminal emulator: absolute cursor moves plus printable text is
# all ratatui needs to place a frame.
grid = [[" "] * COLS for _ in range(ROWS)]
# Background colour per cell, so highlight work can be checked from a real
# terminal stream rather than from a unit test's idea of one.
bg = [[None] * COLS for _ in range(ROWS)]
current_bg = None
row = col = 0
i = 0
csi = re.compile(rb"\x1b\[([0-9;?]*)([A-Za-z])")
while i < len(raw):
    byte = raw[i : i + 1]
    if byte == b"\x1b":
        match = csi.match(raw, i)
        if match:
            params, cmd = match.group(1), match.group(2)
            if params.startswith(b"?"):
                # Private modes (alt screen, cursor visibility) need no state.
                i = match.end()
                continue
            nums = [int(p) if p else 0 for p in params.split(b";")] or [0]
            if cmd == b"H":
                row = (nums[0] - 1) if nums and nums[0] else 0
                col = (nums[1] - 1) if len(nums) > 1 and nums[1] else 0
            elif cmd == b"J":
                grid = [[" "] * COLS for _ in range(ROWS)]
                bg = [[None] * COLS for _ in range(ROWS)]
                row = col = 0
            elif cmd == b"K":
                for x in range(col, COLS):
                    grid[row][x] = " "
                    bg[row][x] = None
            elif cmd == b"m":
                # Only truecolour backgrounds and resets matter here.
                j = 0
                while j < len(nums):
                    if nums[j] == 0:
                        current_bg = None
                    elif nums[j] == 49:
                        current_bg = None
                    elif nums[j] == 48 and j + 4 < len(nums) and nums[j + 1] == 2:
                        current_bg = (nums[j + 2], nums[j + 3], nums[j + 4])
                        j += 4
                    j += 1
            i = match.end()
            continue
        # OSC or other escape: skip to terminator.
        j = raw.find(b"\x07", i)
        k = raw.find(b"\x1b\\", i)
        end = min(x for x in (j, k, i + 2) if x > i)
        i = end + 1
        continue
    if byte == b"\r":
        col = 0
        i += 1
        continue
    if byte == b"\n":
        row = min(row + 1, ROWS - 1)
        i += 1
        continue
    # Decode one UTF-8 character.
    length = 1
    first = raw[i]
    if first >= 0xF0:
        length = 4
    elif first >= 0xE0:
        length = 3
    elif first >= 0xC0:
        length = 2
    try:
        char = raw[i : i + length].decode("utf-8")
    except UnicodeDecodeError:
        char = "?"
    if char.isprintable() and 0 <= row < ROWS and 0 <= col < COLS:
        grid[row][col] = char
        bg[row][col] = current_bg
        width = 2 if ord(char) > 0x2E7F else 1
        # A double-width glyph physically covers the next cell.
        if width == 2 and col + 1 < COLS:
            grid[row][col + 1] = ""
        col += width
    i += length

print("\n".join("".join(r).rstrip() for r in grid))

# Highlight map: readio's two read-aloud washes, as they arrived on the wire.
LIGHT, DEEP = (44, 40, 62), (92, 74, 148)
seen = sorted({cell for r in bg for cell in r if cell})
seen = sorted({cell for r in bg for cell in r if cell})
if not seen:
    print("\n[colour] none on the wire — is NO_COLOR set?")
if any(cell in (LIGHT, DEEP) for r in bg for cell in r):
    print("\n[highlight]  - sentence   # word")
    for y in range(ROWS):
        if not any(bg[y][x] in (LIGHT, DEEP) for x in range(COLS)):
            continue
        marks = "".join(
            "#" if bg[y][x] == DEEP else "-" if bg[y][x] == LIGHT else " "
            for x in range(COLS)
        )
        print(f"  {y:>2}: {marks.rstrip()}")
        print(f"      {''.join(grid[y]).rstrip()}")
exited = "clean" if status == 0 else f"unexpected: {status!r}"
tail = bytes(captured[cut:])
restored = b"\x1b[?1049l" in tail or b"\x1b[?1049l" in bytes(captured)
print(f"\n[exit: {exited}]  [alt-screen restored: {restored}]  [bytes: {len(raw)}]")
