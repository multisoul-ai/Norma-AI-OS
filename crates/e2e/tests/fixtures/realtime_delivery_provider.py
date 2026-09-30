"""Real PTY and Hook boundary; only the explicitly isolated project may be changed."""
import json
import os
from pathlib import Path
import subprocess
import time
import tty
import uuid

root = Path(os.environ["NORMA_NATIVE_FIXTURE_ROOT"]).resolve()
assert Path.cwd().resolve() == root / "Realtime Delivery"
hook = (root / "hook-path").read_text().strip()
session = str(uuid.uuid4())
cloud = (root / "cloud-fixture").is_file()


def emit(name, **fields):
    subprocess.run([hook, "forward", "--source", "codex"],
                   input=json.dumps({"hook_event_name": name, "session_id": session,
                                     **fields}).encode(), check=True)


tty.setraw(0)
os.write(1, b"\x1b[?2004hmodel: fixture\r\n> Ask Codex to do anything\r\n")
emit("SessionStart")
buffer = b""
while True:
    chunk = os.read(0, 8192)
    if not chunk:
        break
    os.write(1, chunk)
    buffer += chunk
    if b"\r" not in buffer:
        continue
    prompt, buffer = buffer.split(b"\r", 1)
    prompt = prompt.replace(b"\x1b[200~", b"").replace(b"\x1b[201~", b"").decode()
    assert (cloud and "result.txt" in prompt and "云端验证完成" in prompt) or prompt in (
        "创建 result.txt，内容为 realtime complete", "添加 queue second 到 result.txt"
    ), "unexpected or cancelled task entered PTY"
    turn = str(uuid.uuid4())
    with (root / "turns.jsonl").open("a") as log:
        log.write(json.dumps({"turn": turn, "prompt": prompt}) + "\n")
    emit("UserPromptSubmit", turn_id=turn, prompt=prompt)
    if cloud or prompt.startswith("创建"):
        (root / "submitted").write_text(prompt)
        Path("result.txt").write_text("云端验证完成\n" if cloud else "realtime complete\n")
        # 120s = 3 tools × 20s + hangup/rebind(30s) + scheduling margin(30s).
        deadline = time.monotonic() + 120
        while not (root / "release-first").exists():
            assert time.monotonic() < deadline, "first task release timed out"
            time.sleep(0.05)
    else:
        with Path("result.txt").open("a") as result:
            result.write("queue second\n")
    emit("Stop", turn_id=turn, last_assistant_message="已创建 result.txt，并核对内容。")
    os.write(1, b"\r\nCompleted: result.txt\r\n> Ask Codex to do anything\r\n")
