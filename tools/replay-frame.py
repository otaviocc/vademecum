#!/usr/bin/env python3
"""Replay a captured terminal stream into a character grid.

    { sleep 0.8; printf 'q'; } | script -q /dev/null \\
        sh -c 'stty rows 20 cols 92; ./target/debug/vademecum README.md' > frame.bin
    python3 tools/replay-frame.py 20 92 frame.bin

Stripping the escapes loses the layout: ratatui positions the cursor and paints
runs, and never emits the spaces in between, so a stripped capture is a wall of
run-together words. Only replaying the cursor moves recovers the frame — which
also means a frame assembled from several partial repaints comes out right.
"""
import re, sys, unicodedata

rows, cols = int(sys.argv[1]), int(sys.argv[2])
data = open(sys.argv[3], 'rb').read().decode('utf-8', 'replace')
grid = [[' '] * cols for _ in range(rows)]
r = c = 0
i = 0
csi = re.compile(r'\x1b\[([0-9;?]*)([a-zA-Z])')
while i < len(data):
    ch = data[i]
    if ch == '\x1b':
        m = csi.match(data, i)
        if not m:
            i += 1
            continue
        args, cmd = m.group(1), m.group(2)
        nums = [int(x) for x in args.split(';') if x.isdigit()]
        if cmd == 'H':
            r = (nums[0] - 1) if nums else 0
            c = (nums[1] - 1) if len(nums) > 1 else 0
        elif cmd == 'J' and nums[:1] == [2]:
            grid = [[' '] * cols for _ in range(rows)]
        elif cmd == 'K':
            if 0 <= r < rows:
                for x in range(c, cols):
                    grid[r][x] = ' '
        elif cmd == 'C':
            c += nums[0] if nums else 1
        i = m.end()
        continue
    if ch == '\n':
        r, c = r + 1, 0
    elif ch == '\r':
        c = 0
    elif ch >= ' ':
        if 0 <= r < rows and 0 <= c < cols:
            grid[r][c] = ch
        # East-Asian wide characters occupy two cells.
        c += 2 if unicodedata.east_asian_width(ch) in ('W', 'F') else 1
    i += 1
for line in grid:
    print(''.join(line).rstrip())
