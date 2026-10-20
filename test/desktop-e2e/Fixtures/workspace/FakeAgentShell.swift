import Foundation

/// 为 Agent 卡片提供可重复的假 CLI：通过真实的 Hook 接口上报运行中与待办进度。
/// 登录 shell 读取 ZDOTDIR 下的 .zshrc，把假 `claude`/`codex` 放到 PATH 最前。
public enum FakeAgentShell {
    public static func install() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-fake-agents-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: root.appendingPathComponent(".zshrc"))
        try write(claude, to: bin.appendingPathComponent("claude"))
        try write("#!/bin/sh\nexec sleep 3600\n", to: bin.appendingPathComponent("codex"))
        return root
    }

    private static func write(_ text: String, to url: URL) throws {
        try Data(text.utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    private static let claude = """
    #!/usr/bin/env python3
    import json, os, time, urllib.request
    node = int(os.environ["MULTISOUL_NODE_ID"])
    def post(event, tool, raw):
        raw = dict(raw, hook_event_name=event, session_id="e2e-session", prompt_id="e2e-turn")
        body = {"node_id": node, "profile": "Claude", "provider_event": event,
                "session_id": "e2e-session", "turn_id": "e2e-turn", "tool_name": tool,
                "message": None, "raw_json": raw}
        request = urllib.request.Request(
            os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
    def create(i, title):
        post("PostToolUse", "TaskCreate", {"tool_name": "TaskCreate", "tool_use_id": "c" + i,
            "tool_input": {"subject": title, "activeForm": title},
            "tool_response": {"task": {"id": i, "subject": title}}})
    def update(i, a, b):
        post("PostToolUse", "TaskUpdate", {"tool_name": "TaskUpdate", "tool_use_id": i + a + b,
            "tool_input": {"taskId": i, "status": b},
            "tool_response": {"success": True, "taskId": i, "statusChange": {"from": a, "to": b}, "updatedFields": ["status"]}})
    time.sleep(2)
    post("UserPromptSubmit", None, {})
    create("1", "整理需求")
    create("2", "原型交互验收")
    create("3", "发布说明")
    update("1", "pending", "in_progress")
    update("1", "in_progress", "completed")
    update("2", "pending", "in_progress")
    time.sleep(3600)
    """
}
