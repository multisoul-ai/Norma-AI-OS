import Foundation

/// 后台大脑验收用的夹具：隔离的大脑目录、假 `claude`（完成时上报 Stop）。
public enum BrainFixture {
    /// 建一个隔离的大脑目录：config.json 开启大脑，`adapters` 指向已装好 ACP 适配器的目录。
    /// 适配器目录由环境变量 NORMA_BRAIN_E2E_ADAPTERS 给出，默认 `~/.norma/brain/adapters`；
    /// NORMA_BRAIN_E2E_BACKEND=codex 或 opencode 时换后端。
    public static func installBrainDirectory() throws -> URL {
        let adapters = ProcessInfo.processInfo.environment["NORMA_BRAIN_E2E_ADAPTERS"]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".norma/brain/adapters", isDirectory: true)
        // NORMA_BRAIN_E2E_BACKEND=codex 时改用 Codex 适配器（codex-acp）。
        // NORMA_BRAIN_E2E_BACKEND=opencode 时用 PATH 里的 opencode，不检查适配器目录。
        let chosen = ProcessInfo.processInfo.environment["NORMA_BRAIN_E2E_BACKEND"] ?? "claude"
        let codex = chosen == "codex"
        if chosen != "opencode" {
            let bin = adapters.appendingPathComponent(
                codex ? "node_modules/.bin/codex-acp" : "node_modules/.bin/claude-agent-acp")
            guard FileManager.default.isExecutableFile(atPath: bin.path) else {
                throw BrainFixtureError.missingAdapter(bin.path)
            }
        }
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let backend = ["codex", "opencode"].contains(chosen) ? chosen : "claude"
        try Data(#"{"enabled": true, "backend": "\#(backend)", "strategy": "speed"}"#.utf8)
            .write(to: root.appendingPathComponent("config.json"))
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent("adapters"), withDestinationURL: adapters)
        return root
    }

    /// 脚本化大脑收到完成事件后做什么。
    public enum ScriptedMode: String {
        /// 直接派活（`norma brain assign`），配置里打开“自行派任务”。
        case assign
        /// 只提议（`norma brain propose`），等用户在概览里确认。
        case propose
    }

    /// 脚本化的大脑：不用真实模型，收到完成事件后通过权限闸门执行 `norma brain assign` 或 `propose`，
    /// 让概览里的记录可重复地出现。适配器放在 `adapters/node_modules/.bin/claude-agent-acp`。
    public static func installScriptedBrainDirectory(mode: ScriptedMode = .assign) throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-scripted-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("adapters/node_modules/.bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let assign = mode == .assign ? "true" : "false"
        try Data(#"{"enabled": true, "backend": "claude", "strategy": "speed", "autonomy": {"assign_followups": \#(assign)}}"#.utf8)
            .write(to: root.appendingPathComponent("config.json"))
        try Data(mode.rawValue.utf8).write(to: root.appendingPathComponent("mode"))
        let script = bin.appendingPathComponent("claude-agent-acp")
        try Data(scriptedAdapter.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return root
    }

    private static let scriptedAdapter = """
    #!/usr/bin/env python3
    import json, os, re, subprocess, sys
    def send(o): sys.stdout.write(json.dumps(o) + "\\n"); sys.stdout.flush()
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
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
    done = [False]
    while True:
        m = read(); method, mid = m.get("method"), m.get("id")
        if method == "initialize": send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1, "agentCapabilities": {}}})
        elif method == "session/new": send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "scripted"}})
        elif method == "session/set_mode": send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "session/prompt":
            text = "".join(p.get("text", "") for p in m["params"]["prompt"])
            with open(os.path.join(os.environ.get("NORMA_BRAIN_DIR", "/tmp"), "prompt.log"), "a") as f:
                f.write(text.replace("\\n", " ⏎ ") + "\\n")
            found = re.search(r"`([^`\\s]+) brain agents`", text)
            if ("【事件】" in text or "【静默记录】" in text) and found and not done[0]:
                done[0] = True
                exe = found.group(1)
                mode = open(os.path.join(os.environ.get("NORMA_BRAIN_DIR", "/tmp"), "mode")).read().strip()
                marker = os.path.join(os.environ.get("NORMA_BRAIN_DIR", "/tmp"), "proposed.once")
                if mode == "propose" and not os.path.exists(marker):
                    for args in (["Bella", "--title", "补登录页测试", "--task", "补上登录页的测试，cargo test 通过即完成", "--why", "登录页还没有测试"],
                                 ["Alan", "--title", "写登录接口文档", "--task", "为登录接口写使用文档", "--why", "接口刚改过"]):
                        argv = [exe, "brain", "propose"] + args
                        if permission("scripted", " ".join(argv)):
                            result = subprocess.run(argv, env=os.environ, timeout=30, capture_output=True, text=True)
                            with open(os.path.join(os.environ.get("NORMA_BRAIN_DIR", "/tmp"), "propose.out"), "a") as f:
                                f.write(result.stdout + result.stderr + "\\n")
                    open(marker, "w").write("done")
                elif mode != "propose":
                    command = exe + " brain assign Bella --task '补上登录页的测试'"
                    if permission("scripted", command):
                        subprocess.run([exe, "brain", "assign", "Bella", "--task", "补上登录页的测试"], env=os.environ, timeout=30)
                # 再读一次 Alan 的对话：e2e 里的假 Agent 没有手机控制，应当得到如实的“读不了”。
                read_command = exe + " brain history Alan --last 2"
                if permission("scripted", read_command):
                    result = subprocess.run([exe, "brain", "history", "Alan", "--last", "2"], env=os.environ,
                                            timeout=30, capture_output=True, text=True)
                    with open(os.path.join(os.environ.get("NORMA_BRAIN_DIR", "/tmp"), "history.out"), "w") as f:
                        f.write(result.stdout + result.stderr)
            send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
        elif mid is not None: send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "unsupported"}})
    """

    /// 假 `claude`：启动后提交一条提示，几秒后以 Stop 结束，并带最终回复。
    /// `finalMessage` 是第一张卡片完成时给出的最终回复：带“需要你”之类的词会走立即汇报，
    /// 平平的结果走静默池。
    public static func installFakeAgents(
        finalMessage: String = "登录页已经做完，12 个测试全部通过，但需要你确认是否保留旧的登录接口。"
    ) throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-fake-agents-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: root.appendingPathComponent(".zshrc"))
        let script = bin.appendingPathComponent("claude")
        try Data(claude.replacingOccurrences(of: "__FINAL__", with: finalMessage).utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return root
    }

    private static let claude = """
    #!/usr/bin/env python3
    import json, os, re, sys, threading, time, urllib.request
    node = int(os.environ["MULTISOUL_NODE_ID"])
    def receive():
        for line in sys.stdin:
            task = re.sub(r"\\x1b\\[(?:200|201)~", "", line).strip()
            if not task: continue
            path = os.path.join(os.environ["NORMA_BRAIN_DIR"], "received.jsonl")
            with open(path, "a", encoding="utf-8") as f:
                f.write(json.dumps({"node": node, "task": task}, ensure_ascii=False) + "\\n")
    threading.Thread(target=receive, daemon=True).start()
    def post(event, raw):
        raw = dict(raw, hook_event_name=event, session_id="brain-e2e", prompt_id="brain-e2e-turn")
        body = {"node_id": node, "profile": "Claude", "provider_event": event,
                "session_id": "brain-e2e", "turn_id": "brain-e2e-turn", "tool_name": None,
                "message": None, "raw_json": raw}
        request = urllib.request.Request(
            os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
    # 第一张卡片会完成任务；其余卡片一直待命，用来接后续任务。
    if node != 1:
        time.sleep(3600)
    time.sleep(2)
    post("UserPromptSubmit", {"prompt": "实现登录页并补测试"})
    time.sleep(6)
    post("Stop", {"last_assistant_message": "__FINAL__"})
    time.sleep(3600)
    """
}

public enum BrainFixtureError: Error, CustomStringConvertible {
    case missingAdapter(String)

    public var description: String {
        switch self {
        case .missingAdapter(let path):
            return "找不到 ACP 适配器 \(path)；用 NORMA_BRAIN_E2E_ADAPTERS 指向装有 @agentclientprotocol/claude-agent-acp 的目录"
        }
    }
}
