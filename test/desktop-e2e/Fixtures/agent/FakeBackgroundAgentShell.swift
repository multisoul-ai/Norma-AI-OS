import Foundation

/// 后台任务完成判定用的假 Claude。
/// 登录 shell 读取 ZDOTDIR 下的 .zshrc，把假 `claude` 放到 PATH 最前。
/// 假 HOME 里放着 Claude 的「已安装 hook」配置，App 才会把该 profile 当成已武装。
/// 脚本自己向 hook 端点上报，不调用真实 CLI：
/// SessionStart → UserPromptSubmit → PostToolUse(run_in_background) → Stop(background_tasks 非空) →
/// 等到 `settle` 文件 → task-notification → Stop(background_tasks 为空) → `sleep 3600`。
/// 窗口结束前不发 SessionEnd，否则会取消待定的 Done。
public enum FakeBackgroundAgentShell {
    public static let holdBanner = "BG HOLD"
    public static let settledBanner = "BG SETTLED"
    public static let monitorBanner = "GROK MONITOR IDLE"

    public struct Install {
        public let root: URL
        public var home: URL { root.appendingPathComponent("home", isDirectory: true) }
        public var settle: URL { root.appendingPathComponent("settle") }

        /// 让假 Claude 发出结清通知和空的 Stop。
        public func release() throws {
            try Data().write(to: settle)
        }
    }

    public static func install() throws -> Install {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-fake-background-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_FAKE_ROOT=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        let install = Install(root: root)
        try writeHookConfig(
            to: install.home.appendingPathComponent(".claude/settings.json"),
            events: [
                "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                "PermissionRequest", "PostToolUseFailure", "StopFailure", "Stop",
                "SessionEnd", "SubagentStart", "SubagentStop",
            ],
            source: "claude"
        )
        try writeHookConfig(
            to: install.home.appendingPathComponent(".grok/hooks/norma-hook.json"),
            events: ["SessionStart", "UserPromptSubmit", "Notification", "Stop", "StopCancelled", "SessionEnd"],
            source: "grok"
        )
        try write(claude, to: bin.appendingPathComponent("claude"))
        try write(grok, to: bin.appendingPathComponent("grok"))
        return install
    }

    private static func writeHookConfig(to url: URL, events: [String], source: String) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        let command = "/usr/local/bin/norma-hook forward --source \(source)"
        var hooks: [String: Any] = [:]
        for event in events {
            hooks[event] = [["hooks": [["type": "command", "command": command]]]]
        }
        let data = try JSONSerialization.data(withJSONObject: ["hooks": hooks], options: [.sortedKeys])
        try data.write(to: url)
    }

    private static func write(_ text: String, to url: URL) throws {
        try Data((text + "\n").utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    private static let claude = """
    #!/usr/bin/env python3
    import json, os, time, urllib.request
    node = int(os.environ["NORMA_NODE_ID"])
    root = os.environ["NORMA_FAKE_ROOT"]

    def post(event, tool, raw):
        raw = dict(raw, hook_event_name=event, session_id="e2e-bg")
        body = {"node_id": node, "profile": "Claude", "provider_event": event,
                "session_id": "e2e-bg", "turn_id": "e2e-turn", "tool_name": tool,
                "message": None, "raw_json": raw}
        request = urllib.request.Request(
            os.environ["NORMA_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["NORMA_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()

    print("BG HOLD", flush=True)
    post("SessionStart", None, {"source": "startup"})
    post("UserPromptSubmit", None, {"prompt": "run tests in background"})
    post("PostToolUse", "Bash", {
        "tool_name": "Bash",
        "tool_use_id": "toolu_bg",
        "tool_input": {"command": "sleep 30", "run_in_background": True},
        "tool_response": {"backgroundTaskId": "bd0jz0gti"},
    })
    post("Stop", None, {
        "stop_hook_active": False,
        "background_tasks": [{"id": "bd0jz0gti", "type": "shell", "status": "running"}],
        "session_crons": [],
    })
    while not os.path.exists(os.path.join(root, "settle")):
        time.sleep(0.1)
    prompt = (
        "<task-notification>\\n"
        "<task-id>bd0jz0gti</task-id>\\n"
        "<tool-use-id>toolu_bg</tool-use-id>\\n"
        "<status>completed</status>\\n"
        "</task-notification>"
    )
    post("UserPromptSubmit", None, {"prompt": prompt})
    post("Stop", None, {
        "stop_hook_active": False,
        "background_tasks": [],
        "session_crons": [],
    })
    print("BG SETTLED", flush=True)
    time.sleep(3600)
    """

    /// 假 Grok：回合结束前启动常驻 monitor（取证：打包监视器 timeout 3 小时），
    /// Stop 的 backgroundTasks 里只有这个 running monitor，之后进程保持 sleep、不发 SessionEnd。
    private static let grok = """
    #!/usr/bin/env python3
    import json, os, time, urllib.request
    node = int(os.environ["NORMA_NODE_ID"])

    def post(event, raw):
        raw = dict(raw, hook_event_name=event, sessionId="e2e-grok", session_id="e2e-grok")
        body = {"node_id": node, "profile": "Grok", "provider_event": event,
                "session_id": "e2e-grok", "turn_id": None, "tool_name": None,
                "message": None, "raw_json": raw}
        request = urllib.request.Request(
            os.environ["NORMA_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["NORMA_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()

    post("SessionStart", {"source": "new"})
    post("UserPromptSubmit", {"prompt": "package and watch", "promptId": "p1"})
    time.sleep(1.5)
    post("Stop", {
        "reason": "end_turn", "promptId": "p1", "stopHookActive": False,
        "backgroundTasks": [{"id": "m1", "type": "monitor", "status": "running",
                             "description": "watch packaging log"}],
        "sessionCrons": [],
    })
    print("GROK MONITOR IDLE", flush=True)
    time.sleep(3600)
    """
}
