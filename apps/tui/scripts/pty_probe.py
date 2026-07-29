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
# `--trace` prints a token-count-against-time curve at the end. Read-aloud is a
# claim about timing, and a final screenshot cannot show a four-second silence
# in the middle of a paragraph; this can.
TRACE = "--trace" in ARGV
if TRACE:
    ARGV.remove("--trace")

KEYS = {
    "enter": b"\r",
    "esc": b"\x1b",
    "ctrl-c": b"\x03",
    "ctrl-d": b"\x04",
    "ctrl-l": b"\x0c",
    "ctrl-s": b"\x13",
    "ctrl-t": b"\x14",
    "ctrl-o": b"\x0f",
    "ctrl-r": b"\x12",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
    "pgup": b"\x1b[5~",
    "pgdn": b"\x1b[6~",
    "home": b"\x1b[H",
    "end": b"\x1b[F",
    "tab": b"\t",
    # shift+tab, which is how readio cycles its reading mode.
    "btab": b"\x1b[Z",
    "space": b" ",
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
# (byte offset just past a chunk, seconds since start) for every read, so any
# offset in the stream can be dated afterwards.
arrivals = []
started = time.time()


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
            arrivals.append((len(captured), time.time() - started))


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

csi = re.compile(rb"\x1b\[([0-9;?]*)([A-Za-z])")


def emulate(stream, stops=()):
    """Replay `stream` through a very small terminal emulator.

    Absolute cursor moves plus printable text is all ratatui needs to place a
    frame. `stops` is a list of `(byte offset, seconds)`; at each one the screen
    as it stood at that moment is handed to the caller, which is the only way to
    see *when* something appeared. ratatui writes only the cells that changed,
    so a word that stays put — "tok" in the status bar — is on the wire once and
    then never again; nothing short of replaying the stream can date the number
    beside it.
    """
    grid = [[" "] * COLS for _ in range(ROWS)]
    # Background colour per cell, so highlight work can be checked from a real
    # terminal stream rather than from a unit test's idea of one.
    bg = [[None] * COLS for _ in range(ROWS)]
    # Italic per cell: emphasis in a book arrives as SGR 3, and the only way to
    # know it survived wrapping and streaming is to read it back off the wire.
    ital = [[False] * COLS for _ in range(ROWS)]
    # Foreground per cell: readio tints Latin words inside Chinese prose the way
    # a coding agent tints identifiers, and that only shows as an SGR 38;2 run.
    fg = [[None] * COLS for _ in range(ROWS)]
    current_bg = None
    current_fg = None
    current_italic = False
    row = col = 0
    i = 0
    stops = list(stops)
    next_stop = 0
    frames = []
    while i < len(stream):
        while next_stop < len(stops) and stops[next_stop][0] <= i:
            frames.append((stops[next_stop][1], [r[:] for r in grid]))
            next_stop += 1
        byte = stream[i : i + 1]
        if byte == b"\x1b":
            match = csi.match(stream, i)
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
                    ital = [[False] * COLS for _ in range(ROWS)]
                    fg = [[None] * COLS for _ in range(ROWS)]
                    row = col = 0
                elif cmd == b"K":
                    for x in range(col, COLS):
                        grid[row][x] = " "
                        bg[row][x] = None
                        ital[row][x] = False
                        fg[row][x] = None
                elif cmd == b"m":
                    # Only truecolour backgrounds and resets matter here.
                    j = 0
                    while j < len(nums):
                        if nums[j] == 0:
                            current_bg = None
                            current_fg = None
                            current_italic = False
                        elif nums[j] == 3:
                            current_italic = True
                        elif nums[j] == 23:
                            current_italic = False
                        elif nums[j] == 49:
                            current_bg = None
                        elif nums[j] == 48 and j + 4 < len(nums) and nums[j + 1] == 2:
                            current_bg = (nums[j + 2], nums[j + 3], nums[j + 4])
                            j += 4
                        elif nums[j] == 38 and j + 4 < len(nums) and nums[j + 1] == 2:
                            current_fg = (nums[j + 2], nums[j + 3], nums[j + 4])
                            j += 4
                        elif nums[j] == 39:
                            current_fg = None
                        j += 1
                i = match.end()
                continue
            # OSC or other escape: skip to terminator.
            j = stream.find(b"\x07", i)
            k = stream.find(b"\x1b\\", i)
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
        first = stream[i]
        if first >= 0xF0:
            length = 4
        elif first >= 0xE0:
            length = 3
        elif first >= 0xC0:
            length = 2
        try:
            char = stream[i : i + length].decode("utf-8")
        except UnicodeDecodeError:
            char = "?"
        if char.isprintable() and 0 <= row < ROWS and 0 <= col < COLS:
            grid[row][col] = char
            bg[row][col] = current_bg
            fg[row][col] = current_fg
            ital[row][col] = current_italic
            width = 2 if ord(char) > 0x2E7F else 1
            # A double-width glyph physically covers the next cell.
            if width == 2 and col + 1 < COLS:
                grid[row][col + 1] = ""
            col += width
        i += length
    while next_stop < len(stops):
        frames.append((stops[next_stop][1], [r[:] for r in grid]))
        next_stop += 1
    return grid, bg, ital, fg, frames


grid, bg, ital, fg, _ = emulate(raw)

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
# Emphasis map: what the book leaned on, as the terminal received it.
if any(ital[y][x] for y in range(ROWS) for x in range(COLS)):
    print("\n[italic]")
    for y in range(ROWS):
        if not any(ital[y][x] for x in range(COLS)):
            continue
        run = "".join(grid[y][x] for x in range(COLS) if ital[y][x])
        print(f"  {y:>3}:  {run.strip()}")

# Latin runs in prose, tinted like a coding agent tints identifiers.
LATIN = (122, 194, 214)
if any(fg[y][x] == LATIN for y in range(ROWS) for x in range(COLS)):
    print("\n[tinted]")
    for y in range(ROWS):
        run = "".join(grid[y][x] for x in range(COLS) if fg[y][x] == LATIN)
        if run.strip():
            print(f"  {y:>3}:  {run.strip()}")

print(f"\n[exit: {exited}]  [alt-screen restored: {restored}]  [bytes: {len(raw)}]")

if TRACE:
    # The status bar carries the running token count, so replaying the stream
    # with a stop at every read gives a timestamped record of how fast text
    # actually reached the reader — and, more to the point, where it stopped.
    _, _, _, _, frames = emulate(bytes(captured), arrivals)
    curve = {}
    for when, snapshot in frames:
        text = "\n".join("".join(r) for r in snapshot)
        found = re.search(r"([0-9][0-9,]*) tok", text)
        if found:
            curve.setdefault(int(found.group(1).replace(",", "")), when)
    if len(curve) > 1:
        print("\n[trace]  seconds → tokens on screen  (gaps are silences)")
        line, last = [], None
        for count in sorted(curve):
            when = curve[count]
            gap = "" if last is None else f"+{when - last:.1f}"
            line.append(f"{when:5.1f}s {count:>5}tok {gap:>6}")
            last = when
        for n in range(0, len(line), 3):
            print("  " + " | ".join(line[n : n + 3]))
        span = max(curve.values()) - min(curve.values())
        grew = max(curve) - min(curve)
        if span > 0:
            print(f"  overall: {grew} tok in {span:.1f}s = {grew / span:.1f} tok/s")
        stalls = []
        prev = None
        for count in sorted(curve):
            if prev is not None and curve[count] - prev > 1.5:
                stalls.append(f"{prev:.1f}s→{curve[count]:.1f}s")
            prev = curve[count]
        print(f"  stalls over 1.5s: {', '.join(stalls) if stalls else 'none'}")
        print("\n[trace]  seconds → tokens on screen  (gaps are silences)")
        line, last = [], None
        for count in sorted(curve):
            when = curve[count]
            gap = "" if last is None else f"+{when - last:.1f}"
            line.append(f"{when:5.1f}s {count:>5}tok {gap:>6}")
            last = when
        for n in range(0, len(line), 3):
            print("  " + " | ".join(line[n : n + 3]))
        span = max(curve.values()) - min(curve.values())
        grew = max(curve) - min(curve)
        if span > 0:
            print(f"  overall: {grew} tok in {span:.1f}s = {grew / span:.1f} tok/s")
        stalls = []
        prev = None
        for count in sorted(curve):
            if prev is not None and curve[count] - prev > 1.5:
                stalls.append(f"{prev:.1f}s→{curve[count]:.1f}s")
            prev = curve[count]
        print(f"  stalls over 1.5s: {', '.join(stalls) if stalls else 'none'}")
