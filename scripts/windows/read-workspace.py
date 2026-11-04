#!/usr/bin/env python3
"""Read-only observation of the workspace created through native UI input."""
import argparse
import json
from pathlib import Path
import sqlite3
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--database', type=Path, required=True)
parser.add_argument('--name', required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--wait-seconds', type=float, default=0)
args = parser.parse_args()
if not 0 <= args.wait_seconds <= 30:
    parser.error('--wait-seconds must be between 0 and 30')
deadline = time.monotonic() + args.wait_seconds
while True:
    with sqlite3.connect(args.database.resolve().as_uri() + '?mode=ro', uri=True) as connection:
        connection.row_factory = sqlite3.Row
        rows = [dict(row) for row in connection.execute(
            'SELECT * FROM desktop_workspaces WHERE name = ?', (args.name,))]
    if rows or time.monotonic() >= deadline:
        break
    time.sleep(min(0.2, max(0, deadline - time.monotonic())))
args.output.write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding='utf-8')
if len(rows) != 1:
    raise SystemExit(f'Expected exactly one UI-created workspace, found {len(rows)}')
