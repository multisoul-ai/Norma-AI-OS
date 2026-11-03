#!/usr/bin/env python3
"""Read-only observation of the workspace created through native UI input."""
import argparse
import json
from pathlib import Path
import sqlite3

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--database', type=Path, required=True)
parser.add_argument('--name', required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
with sqlite3.connect(args.database.resolve().as_uri() + '?mode=ro', uri=True) as connection:
    connection.row_factory = sqlite3.Row
    rows = [dict(row) for row in connection.execute(
        'SELECT * FROM desktop_workspaces WHERE name = ?', (args.name,))]
args.output.write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding='utf-8')
if len(rows) != 1:
    raise SystemExit(f'Expected exactly one UI-created workspace, found {len(rows)}')
