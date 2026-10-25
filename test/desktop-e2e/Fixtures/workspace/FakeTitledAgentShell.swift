import Foundation

/// 带会话名的假 CLI：经真实 Hook 接口上报提示词，并写出 Claude transcript（ai-title）与
/// Codex session_index.jsonl（thread_name），用来验证卡片标题优先取 CLI 自己的会话名。
/// 第一个启动的 `claude` 会写 ai-title，第二个不写，用来覆盖回退到首条提示词。
public enum FakeTitledAgentShell {
    // 用英文标题：窗口 OCR 对中文偶发误识（如「我」读成「目」），会让断言抖动。
    public static let claudeTitle = "Refactor permission gate"
    public static let codexTitle = "Write self introduction"
    public static let claudePrompt = "Alpha prompt"
    public static let fallbackPrompt = "Beta prompt"
    public static let codexPrompt = "Gamma prompt"

    public struct Install {
        public let root: URL
        public let codexHome: URL
    }

    public static func install() throws -> Install {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-fake-titled-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        let codexHome = root.appendingPathComponent("codex-home", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: codexHome, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_FAKE_ROOT=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        try write(script(profile: "Claude", prompt: nil), to: bin.appendingPathComponent("claude"))
        try write(script(profile: "Codex", prompt: codexPrompt), to: bin.appendingPathComponent("codex"))
        return Install(root: root, codexHome: codexHome)
    }

    private static func write(_ text: String, to url: URL) throws {
        try Data(text.utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    private static func script(profile: String, prompt: String?) -> String {
        """
        #!/usr/bin/env python3
        import json, os, time, urllib.request
        node = int(os.environ["MULTISOUL_NODE_ID"])
        root = os.environ["NORMA_FAKE_ROOT"]
        profile = "\(profile)"
        session = "e2e-%s-%d" % (profile.lower(), node)
        def claim(name):
            try:
                os.close(os.open(os.path.join(root, name), os.O_CREAT | os.O_EXCL))
                return True
            except FileExistsError:
                return False
        if profile == "Claude":
            titled = claim("claude-titled")
            prompt = "\(claudePrompt)" if titled else "\(fallbackPrompt)"
        else:
            titled = True
            prompt = "\(prompt ?? "")"
        # 与真实 CLI 一致：transcript 文件名必须是 <session_id>.jsonl，否则运行时拒收该事件。
        transcript = os.path.join(root, session + ".jsonl")
        open(transcript, "w").write(json.dumps({"type": "user", "message": prompt}) + "\\n")
        def post(event, extra):
            raw = dict(extra, hook_event_name=event, session_id=session, transcript_path=transcript)
            body = {"node_id": node, "profile": profile, "provider_event": event,
                    "session_id": session, "turn_id": "turn-1", "tool_name": None,
                    "message": None, "raw_json": raw}
            request = urllib.request.Request(
                os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
                {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
            urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
        time.sleep(2)
        post("UserPromptSubmit", {"prompt": prompt})
        time.sleep(1)
        # CLI 在一轮进行中才写出会话名，所以 Stop 之前才落盘。
        if titled and profile == "Claude":
            with open(transcript, "a") as f:
                f.write(json.dumps({"type": "ai-title", "aiTitle": "\(claudeTitle)", "sessionId": session}) + "\\n")
        if titled and profile == "Codex":
            with open(os.path.join(os.environ["CODEX_HOME"], "session_index.jsonl"), "a") as f:
                f.write(json.dumps({"id": session, "thread_name": "\(codexTitle)", "updated_at": "2026-10-25T00:00:00Z"}) + "\\n")
        post("Stop", {})
        time.sleep(3600)
        """
    }
}
