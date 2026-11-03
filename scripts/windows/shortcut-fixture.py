#!/usr/bin/env python3
"""Temporarily seed one persisted Quit shortcut on an isolated GitHub runner."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("action", choices=("configure", "restore"))
parser.add_argument("--database", type=Path, required=True)
parser.add_argument("--backup", type=Path, required=True)
args = parser.parse_args()
if os.environ.get("GITHUB_ACTIONS") != "true":
    raise SystemExit("This fixture only runs on GitHub Actions")
if not args.database.is_file():
    raise SystemExit("The startup baseline must initialize the runner database first")

setting = "keyboard.shortcuts.v1"
configured = json.dumps({"overrides": [{"command": "quit_norma_os", "keystroke": "ctrl-a"}]})
with sqlite3.connect(args.database) as connection:
    row = connection.execute(
        "SELECT value, updated_at_unix FROM settings WHERE key = ?", (setting,)
    ).fetchone()
    if args.action == "configure":
        with args.backup.open("x", encoding="utf-8") as backup:
            json.dump(row, backup)
        connection.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at_unix) VALUES (?, ?, ?)",
            (setting, configured, int(time.time())),
        )
        actual = connection.execute("SELECT value FROM settings WHERE key = ?", (setting,)).fetchone()
        if actual != (configured,):
            raise SystemExit("The persisted shortcut fixture did not take effect")
    else:
        if row is None or row[0] != configured:
            raise SystemExit("Shortcut changed outside the fixture; refusing to overwrite it")
        previous = json.loads(args.backup.read_text(encoding="utf-8"))
        if previous is None:
            connection.execute("DELETE FROM settings WHERE key = ?", (setting,))
        else:
            connection.execute(
                "UPDATE settings SET value = ?, updated_at_unix = ? WHERE key = ?",
                (*previous, setting),
            )
        restored = connection.execute(
            "SELECT value, updated_at_unix FROM settings WHERE key = ?", (setting,)
        ).fetchone()
        if restored != (tuple(previous) if previous is not None else None):
            raise SystemExit("The runner's original shortcut must be restored exactly")
