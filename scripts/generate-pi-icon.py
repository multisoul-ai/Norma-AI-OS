#!/usr/bin/env python3
"""Convert the official Pi pixel logo into an integer-rect toolbar SVG.

Usage: generate-pi-icon.py --logo-ts <pi-logo.ts> --output <toolbar-pi.svg>

The input is the official `pi-logo.ts` (MIT, (c) 2025 Mario Zechner) from
https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/modes/interactive/components/pi-logo.ts
whose doc comment lists the 4x4 pixel grid. Every filled pixel becomes one 4x4 `rect` on a 24x24
canvas (offset 4), drawn in `currentColor` to match the other monochrome toolbar icons.
"""
import argparse
import re
import sys

CANVAS = 24
PIXEL = 4
KNOWN = {"coral", "blue", "yellow"}


def parse_grid(source: str) -> list[list[bool]]:
    rows = []
    for line in source.splitlines():
        tokens = line.strip().lstrip("*").split()
        if len(tokens) == 4 and all(t == "." or t in KNOWN for t in tokens):
            rows.append([t != "." for t in tokens])
    if len(rows) != 4:
        raise ValueError(f"expected 4 grid rows in pi-logo.ts comment, found {len(rows)}")
    return rows


def render(grid: list[list[bool]]) -> str:
    size = PIXEL * len(grid)
    offset = (CANVAS - size) // 2
    out = [
        f'<svg width="{CANVAS}" height="{CANVAS}" viewBox="0 0 {CANVAS} {CANVAS}" '
        'xmlns="http://www.w3.org/2000/svg" fill="currentColor">',
        "  <!-- Pi pixel logo, converted from the official pi-logo.ts (MIT, (c) 2025 Mario Zechner): "
        "https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/modes/interactive/components/pi-logo.ts -->",
    ]
    for row, cells in enumerate(grid):
        for col, filled in enumerate(cells):
            if filled:
                out.append(
                    f'  <rect x="{offset + col * PIXEL}" y="{offset + row * PIXEL}" '
                    f'width="{PIXEL}" height="{PIXEL}"/>'
                )
    out.append("</svg>")
    return "\n".join(out) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--logo-ts", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    with open(args.logo_ts, encoding="utf-8") as handle:
        grid = parse_grid(handle.read())
    with open(args.output, "w", encoding="utf-8") as handle:
        handle.write(render(grid))
    return 0


if __name__ == "__main__":
    sys.exit(main())
