#!/usr/bin/env python3
"""Drive local notification events while XCTest performs native iOS UI actions.
Requires an already running mobile_notification_native notification-ui fixture.
This uses simctl injection; it never contacts APNs and is not Computer Use evidence.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("fixture", type=Path)
parser.add_argument("--simulator", required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
fixture = args.fixture.resolve(strict=True)
event = json.loads((fixture / "event.apns").read_text())
assert (fixture / "ui-ready").exists(), "native provisioning must finish first"
for marker in ("warm-push-ready", "replacement-push-ready", "nested-push-ready", "push-ready", "stale-push-ready", "ui-stale"):
    (fixture / marker).unlink(missing_ok=True)
stale = json.loads(json.dumps(event))
stale["aps"]["alert"]["body"] = "一项旧窗口任务已完成"
stale["norma"]["event_id"] = "22222222-3333-4444-8555-666666666666"
(fixture / "stale.apns").write_text(json.dumps(stale, ensure_ascii=False))
for name, body in (("warm", "设置中的验收任务已完成"), ("nested", "输入中的验收任务已完成")):
    payload = json.loads(json.dumps(event))
    payload["aps"]["alert"]["body"] = body
    (fixture / f"{name}.apns").write_text(json.dumps(payload, ensure_ascii=False))
replacement = json.loads(json.dumps(event))
replacement["aps"]["alert"]["body"] = "另一项任务的提醒"
replacement["norma"]["route_id"] = "missing-route"
(fixture / "replacement.apns").write_text(json.dumps(replacement, ensure_ascii=False))
log = fixture / "notification-clicks.log"
command = ["xcodebuild", "-project", str(root / "ios/Norma.xcodeproj"), "-scheme", "Norma",
           "-destination", f"platform=iOS Simulator,id={args.simulator}",
           "-derivedDataPath", str(root / "ios/DerivedData"),
           f"NORMA_TEST_QR={fixture / 'offer.json'}", "NORMA_TEST_MODE=notification-ui",
           "-only-testing:NormaUITests/NotificationFlowUITests/testNotificationColdLaunchAndStaleWindow", "test"]
with log.open("w") as output:
    child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
    # 240s = five native notification paths (up to 160s waits) + 80s Xcode/teardown margin.
    deadline = time.monotonic() + 240
    sent = set()
    try:
        while child.poll() is None:
            for marker, filename in (("warm-push-ready", "warm.apns"), ("replacement-push-ready", "replacement.apns"), ("nested-push-ready", "nested.apns"), ("push-ready", "event.apns"), ("stale-push-ready", "stale.apns")):
                if marker not in sent and (fixture / marker).exists():
                    subprocess.run(["xcrun", "simctl", "push", args.simulator, "com.norma-ai.os.ios", str(fixture / filename)], check=True, timeout=15)
                    sent.add(marker)
            if time.monotonic() >= deadline:
                raise TimeoutError("native notification UI timed out")
            time.sleep(0.1)  # 100ms only for this explicit fixture; not an application poller.
    finally:
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait()
print(log.read_text())
assert child.returncode == 0, f"native UI checks failed; see {log}"
assert sent == {"warm-push-ready", "replacement-push-ready", "nested-push-ready", "push-ready", "stale-push-ready"}, "all five notifications must actually be injected"
