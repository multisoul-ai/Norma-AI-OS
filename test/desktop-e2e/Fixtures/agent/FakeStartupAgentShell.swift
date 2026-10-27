import Foundation

/// 启动看门狗用的假 CLI 与假 HOME。登录 shell 读取 ZDOTDIR 下的 .zshrc，把假 `codex`/`claude` 放到 PATH 最前。
/// - `codex`：启动即画出首屏（`CODEX BANNER`），但在 `release` 文件出现前不发任何 hook，
///   复现真实 Codex 联网失败时「首屏有、SessionStart 没有」的状态；`release` 出现后才发 SessionStart。
/// - `claude`：启动后立即发 SessionStart（模拟健康的 CLI），用来验证负向：不误报。
/// - 假 HOME 里放着 codex/claude 的「已安装 hook」配置，App 才会武装对应 profile。
public enum FakeStartupAgentShell {
    public static let codexBanner = "CODEX BANNER"
    public static let claudeBanner = "CLAUDE BANNER"

    public struct Install {
        public let root: URL
        public var home: URL { root.appendingPathComponent("home", isDirectory: true) }
        public var release: URL { root.appendingPathComponent("release") }

        /// 让假 codex 发出 SessionStart。
        public func releaseCodex() throws {
            try Data().write(to: release)
        }
    }

    public static func install() throws -> Install {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-fake-startup-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_FAKE_ROOT=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        let install = Install(root: root)
        try writeHookConfig(
            to: install.home.appendingPathComponent(".codex/hooks.json"),
            source: "codex",
            events: ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", "SubagentStart", "SubagentStop"]
        )
        try writeHookConfig(
            to: install.home.appendingPathComponent(".claude/settings.json"),
            source: "claude",
            events: ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "PostToolUseFailure", "StopFailure", "Stop", "SessionEnd", "SubagentStart", "SubagentStop"]
        )
        try write(
            """
            #!/bin/sh
            echo "\(codexBanner)"
            while [ ! -f "$NORMA_FAKE_ROOT/release" ]; do sleep 0.2; done
            /usr/bin/python3 "$NORMA_FAKE_ROOT/bin/send-session-start" Codex
            exec sleep 3600
            """,
            to: bin.appendingPathComponent("codex")
        )
        try write(
            """
            #!/bin/sh
            echo "\(claudeBanner)"
            /usr/bin/python3 "$NORMA_FAKE_ROOT/bin/send-session-start" Claude
            exec sleep 3600
            """,
            to: bin.appendingPathComponent("claude")
        )
        try write(sendSessionStart, to: bin.appendingPathComponent("send-session-start"))
        return install
    }

    private static func writeHookConfig(to url: URL, source: String, events: [String]) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        let command = "/usr/local/bin/multisoul-agent-hook forward --source \(source)"
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

    private static let sendSessionStart = """
    #!/usr/bin/env python3
    import json, os, sys, urllib.request
    profile = sys.argv[1]
    body = {"node_id": int(os.environ["MULTISOUL_NODE_ID"]), "profile": profile,
            "provider_event": "SessionStart", "session_id": "e2e-session", "turn_id": None,
            "tool_name": None, "message": None,
            "raw_json": {"hook_event_name": "SessionStart", "session_id": "e2e-session", "source": "startup"}}
    request = urllib.request.Request(
        os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
        {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
    urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
    """
}
