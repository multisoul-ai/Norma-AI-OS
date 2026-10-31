import Foundation

/// Cmd+K 聊天验收夹具：会回复用户的脚本化幕僚；收到「【用户】…」后经权限闸门执行 `norma brain report`。
public enum BrainDockChatFixture {
    public static func installBrainDirectory() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-dock-chat-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("adapters/node_modules/.bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data(#"{"enabled": true, "backend": "claude", "strategy": "speed"}"#.utf8)
            .write(to: root.appendingPathComponent("config.json"))
        let script = bin.appendingPathComponent("claude-agent-acp")
        try Data(adapter.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return root
    }

    /// 脚本化适配器：把每个收到的用户请求原样写进 user-requests.log，并回一条 report。
    private static let adapter = """
    #!/usr/bin/env python3
    import json, os, re, subprocess, sys
    def send(o): sys.stdout.write(json.dumps(o) + "\\n"); sys.stdout.flush()
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
    directory = os.environ.get("NORMA_BRAIN_DIR", "/tmp")
    exe = [None]
    nid = [100]
    def permission(session, command):
        nid[0] += 1; rid = nid[0]
        send({"jsonrpc": "2.0", "id": rid, "method": "session/request_permission", "params": {
            "sessionId": session,
            "toolCall": {"toolCallId": "t", "name": "Bash", "kind": "execute", "rawInput": {"command": command}},
            "options": [{"optionId": "allow-once", "kind": "allow_once"}, {"optionId": "reject", "kind": "reject_once"}]}})
        while True:
            m = read()
            if m.get("id") == rid: return m["result"]["outcome"].get("optionId") == "allow-once"
    while True:
        m = read(); method, mid = m.get("method"), m.get("id")
        if method == "initialize": send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1, "agentCapabilities": {}}})
        elif method == "session/new": send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "dock-chat"}})
        elif method == "session/set_mode": send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "session/prompt":
            text = "".join(p.get("text", "") for p in m["params"]["prompt"])
            found = re.search(r"`([^`\\s]+) brain agents`", text)
            if found: exe[0] = found.group(1)
            asked = re.search(r"【用户】(.*?)\\n", text)
            if asked and exe[0]:
                with open(os.path.join(directory, "user-requests.log"), "a") as f:
                    f.write(asked.group(1) + "\\n")
                reply = "幕僚收到：" + asked.group(1)
                argv = [exe[0], "brain", "report", "--text", reply, "--priority", "now"]
                if permission("dock-chat", " ".join(argv)):
                    subprocess.run(argv, env=os.environ, timeout=30)
            send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
        elif mid is not None: send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "unsupported"}})
    """
}
