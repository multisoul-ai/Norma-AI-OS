"""Isolated real Codex launcher and a tee of actual CLI Hooks; never fabricates turns."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(os.environ["NORMA_NATIVE_FIXTURE_ROOT"]).resolve()
assert root.parent == Path("/tmp").resolve() and root.name.startswith("norma-doubao-l3.")
assert (root / "real-cli").is_file(), "explicit real CLI fixture required"

if sys.argv[1:] == ["hook"]:
    raw = sys.stdin.buffer.read()
    event = json.loads(raw)
    # Only record non-secret lifecycle fields, then forward the unchanged provider payload.
    record = {k: event[k] for k in ("hook_event_name", "session_id", "turn_id", "prompt") if k in event}
    with (root / "hooks.jsonl").open("a") as log:
        log.write(json.dumps(record) + "\n")
    if event.get("hook_event_name") == "UserPromptSubmit":
        assert event.get("turn_id") and event.get("session_id"), "real provider identity required"
        with (root / "turns.jsonl").open("a") as log:
            log.write(json.dumps({"turn": event["turn_id"], "session": event["session_id"],
                                  "prompt": event.get("prompt")}) + "\n")
    hook = (root / "hook-path").read_text().strip()
    subprocess.run([hook, "forward", "--source", "codex"], input=raw, check=True)
else:
    assert not sys.argv[1:], "test launcher does not accept arbitrary CLI flags"
    assert Path.cwd().resolve() == root / "Realtime Delivery", "isolated project only"
    executable = (root / "codex-path").read_text().strip()
    # The production shell's full-access arguments are not forwarded. Keep normal Hook trust.
    args = [executable, "--sandbox", "workspace-write", "--ask-for-approval", "never",
            "-c", "disable_paste_burst=true", "--no-alt-screen"]
    (root / "cli-launch.json").write_text(json.dumps({"program": executable, "args": args[1:]}))
    os.execve(executable, args, {**os.environ, "CODEX_HOME": str(root / "codex-config")})
