#!/usr/bin/env python3
"""Native regression: compile the production DisplayLink, then count real heap objects.

Requires macOS with an active display and permission for `heap` to inspect our
own probe process. No production Norma process or user session is modified.
"""
import os
import re
import selectors
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "vendor/gpui-0.2.2/tests/display_link_lifecycle/Cargo.toml"


def snapshot(pid):
    # Give native inspection 60 seconds; exceeding it is an ENV/failure, not a pass.
    result = subprocess.run(["heap", "-s", "--noContent", str(pid)], capture_output=True, text=True, check=True, timeout=60)
    counts = {}
    for name in ("CVDisplayLink", "dispatch_source_t", "NSMachPort"):
        match = re.search(rf"^\s*(\d+)\s+\d+\s+[\d.]+\s+{name}\s", result.stdout, re.M)
        if match is None:
            raise RuntimeError(f"heap did not report {name}; native inspection unavailable")
        counts[name] = int(match[1])
    return counts


def ready(proc):
    # Each exercise pumps < 1 second; 30 seconds allows slow CI, but bounds deadlocks.
    with selectors.DefaultSelector() as selector:
        selector.register(proc.stdout, selectors.EVENT_READ)
        if not selector.select(timeout=30) or proc.stdout.readline().strip() != "READY":
            raise RuntimeError("native callback/lifecycle probe failed or timed out")


def main():
    with tempfile.TemporaryDirectory(prefix="norma-display-link-") as target:
        env = dict(os.environ, CARGO_TARGET_DIR=target, CARGO_BUILD_BUILD_DIR=target, CARGO_INCREMENTAL="0")
        subprocess.run(["cargo", "build", "--manifest-path", str(MANIFEST), "--locked", "--quiet"], env=env, check=True, timeout=600)
        proc = subprocess.Popen([str(Path(target) / "debug/display-link-lifecycle")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            ready(proc)
            before = snapshot(proc.pid)
            proc.stdin.write("churn\n")
            proc.stdin.flush()
            ready(proc)
            after = snapshot(proc.pid)
            print(f"native resources: before={before}, after={after}")
            # These are exact owned-object counts, not noisy RSS thresholds.
            if after["CVDisplayLink"] != before["CVDisplayLink"]:
                raise AssertionError("CVDisplayLink count grew after 64 window closes")
            if after["dispatch_source_t"] != before["dispatch_source_t"]:
                raise AssertionError("window dispatch sources were not reclaimed")
            if after["NSMachPort"] != before["NSMachPort"]:
                raise AssertionError("display-link Mach ports accumulated")
            proc.stdin.write("quit\n")
            proc.stdin.flush()
            proc.wait(timeout=10)
            if proc.returncode:
                raise RuntimeError("native probe failed at exit")
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.wait()


if __name__ == "__main__":
    main()
