import Foundation

/// 后台大脑验收用的夹具：隔离的大脑目录、假 `claude`（完成时上报 Stop）。
public enum BrainFixture {
    /// 建一个隔离的大脑目录：config.json 开启大脑，`adapters` 指向已装好 ACP 适配器的目录。
    /// 适配器目录由环境变量 NORMA_BRAIN_E2E_ADAPTERS 给出，默认 `~/.norma/brain/adapters`。
    public static func installBrainDirectory() throws -> URL {
        let adapters = ProcessInfo.processInfo.environment["NORMA_BRAIN_E2E_ADAPTERS"]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".norma/brain/adapters", isDirectory: true)
        let bin = adapters.appendingPathComponent("node_modules/.bin/claude-agent-acp")
        guard FileManager.default.isExecutableFile(atPath: bin.path) else {
            throw BrainFixtureError.missingAdapter(bin.path)
        }
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(#"{"enabled": true, "backend": "claude", "strategy": "speed"}"#.utf8)
            .write(to: root.appendingPathComponent("config.json"))
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent("adapters"), withDestinationURL: adapters)
        return root
    }

    /// 假 `claude`：启动后提交一条提示，几秒后以 Stop 结束，并带最终回复。
    public static func installFakeAgents() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-fake-agents-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: root.appendingPathComponent(".zshrc"))
        let script = bin.appendingPathComponent("claude")
        try Data(claude.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return root
    }

    private static let claude = """
    #!/usr/bin/env python3
    import json, os, time, urllib.request
    node = int(os.environ["MULTISOUL_NODE_ID"])
    def post(event, raw):
        raw = dict(raw, hook_event_name=event, session_id="brain-e2e", prompt_id="brain-e2e-turn")
        body = {"node_id": node, "profile": "Claude", "provider_event": event,
                "session_id": "brain-e2e", "turn_id": "brain-e2e-turn", "tool_name": None,
                "message": None, "raw_json": raw}
        request = urllib.request.Request(
            os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
            {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
        urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
    time.sleep(2)
    post("UserPromptSubmit", {"prompt": "实现登录页并补测试"})
    time.sleep(2)
    post("Stop", {"last_assistant_message": "登录页已经做完，12 个测试全部通过。"})
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
