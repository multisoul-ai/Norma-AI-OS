import Foundation

/// 岛内幕僚验收夹具：脚本化 ACP 适配器把每个提示原样记进 prompts.log，
/// 并经权限闸门执行 `norma brain report --kind reply` 回复；真实后端模式只链接已安装的适配器。
public enum IslandStaffFixture {
    public static func install(real: Bool) throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-island-staff-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(#"{"enabled": true, "backend": "claude", "strategy": "speed", "speak": false}"#.utf8)
            .write(to: root.appendingPathComponent("config.json"))
        if real {
            // 只读链接用户已安装的适配器；不复制、不修改其凭据或配置。
            let home = FileManager.default.homeDirectoryForCurrentUser
            try FileManager.default.createSymbolicLink(
                at: root.appendingPathComponent("adapters"),
                withDestinationURL: home.appendingPathComponent(".norma/brain/adapters"))
            return root
        }
        let bin = root.appendingPathComponent("adapters/node_modules/.bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let script = bin.appendingPathComponent("claude-agent-acp")
        try Data(adapter.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return root
    }

    public static func prompts(_ root: URL) -> String {
        (try? String(contentsOf: root.appendingPathComponent("prompts.log"), encoding: .utf8)) ?? ""
    }

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
        elif method == "session/new": send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "island-staff"}})
        elif method == "session/set_mode": send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "session/prompt":
            text = "".join(p.get("text", "") for p in m["params"]["prompt"])
            with open(os.path.join(directory, "prompts.log"), "a") as f:
                f.write(text.replace("\\n", " ⏎ ") + "\\n")
            found = re.search(r"`([^`\\s]+) brain agents`", text)
            if found: exe[0] = found.group(1)
            reply = None
            ident = re.search(r"【事件来源】(\\{.*?\\})", text)
            scope = json.loads(ident.group(1)) if ident else {}
            asked = re.search(r"【用户】(.*?)\\n", text)
            if asked: reply = "幕僚收到：" + asked.group(1)
            elif "【用户要求汇报】" in text: reply = "目前有一个智能体在运行。没有失败，也没有需要你处理的事。"
            if reply and exe[0]:
                argv = [exe[0], "brain", "--workspace", scope.get("workspace_id", ""), "--route", scope.get("route_id", ""), "report", "--kind", "reply", "--text", reply, "--priority", "now"]
                if permission("island-staff", " ".join(argv)):
                    subprocess.run(argv, env=os.environ, timeout=30)
            send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
        elif mid is not None: send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "unsupported"}})
    """
}
