import Foundation

/// 保持同一 PTY 存活，记录完整 bracketed paste；Ctrl+C 仅终止当前任务。
public enum InterruptibleAgentShell {
    public static func install() throws -> FakeStartupAgentShell.Install {
        let install = try FakeStartupAgentShell.install()
        let script = install.root.appendingPathComponent("bin/claude")
        try Data(program.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return install
    }

    public static func messages(_ install: FakeStartupAgentShell.Install) -> [String] {
        let data = (try? String(contentsOf: install.root.appendingPathComponent("messages.jsonl"), encoding: .utf8)) ?? ""
        return data.split(separator: "\n").compactMap {
            try? JSONDecoder().decode(String.self, from: Data($0.utf8))
        }
    }

    private static let program = #"""
    #!/usr/bin/python3
    import json, os, sys, threading, time, tty, urllib.request
    tty.setraw(sys.stdin.fileno())
    root = os.environ["NORMA_FAKE_ROOT"]
    def hook(event, message=None):
        body = {"node_id": int(os.environ["MULTISOUL_NODE_ID"]), "profile": "Claude",
                "provider_event": event, "session_id": "interruptible-session", "turn_id": None,
                "tool_name": "Bash" if event == "PermissionRequest" else None, "message": message,
                "raw_json": {"hook_event_name": event, "session_id": "interruptible-session", "prompt": message}}
        request = urllib.request.Request(os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
    def output(text):
        os.write(1, (text + "\r\n").encode())
    os.write(1, b"\x1b[?2004h")
    hook("SessionStart")
    output("INTERRUPTIBLE AGENT READY")
    with open(root + "/identity.json", "w") as f:
        json.dump({"pid": os.getpid(), "node_id": int(os.environ["MULTISOUL_NODE_ID"]), "session_id": "interruptible-session"}, f)
    def request_approval():
        while not os.path.exists(root + "/request-approval"): time.sleep(0.05)
        hook("PermissionRequest")
        output("APPROVAL WAIT")
    threading.Thread(target=request_approval, daemon=True).start()
    draft, paste, count = bytearray(), False, 0
    while True:
        ch = os.read(0, 1)
        if not ch: break
        if ch == b"\x1b":
            sequence = ch
            while len(sequence) < 6:
                part = os.read(0, 6 - len(sequence))
                if not part: break
                sequence += part
            if sequence == b"\x1b[200~": paste = True
            elif sequence == b"\x1b[201~": paste = False
            continue
        if ch == b"\x03":
            draft.clear()
            output("TASK INTERRUPTED")
            continue
        if ch in (b"\r", b"\n") and not paste:
            message = draft.decode("utf-8")
            draft.clear()
            if not message.strip(): continue
            count += 1
            with open(root + "/messages.jsonl", "a") as f: f.write(json.dumps(message) + "\n")
            hook("UserPromptSubmit", message)
            output(message.replace("\n", "\r\n"))
            with open(root + "/executed-" + str(count), "w") as f: f.write(message)
            output("EXECUTED " + str(count))
        else:
            draft.extend(ch)
    """#
}
