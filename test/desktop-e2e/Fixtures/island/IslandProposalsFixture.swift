import CryptoKit
import E2EKit
import Foundation

/// 提议验收夹具：空闲的脚本 Agent，以及按项目身份写入的提议文件。
public enum IslandProposalsFixture {
    /// 启动后立刻结束这一轮，让名册里的 Agent 处于可派发状态。
    public static func installShell() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp")
            .appendingPathComponent("norma-island-proposals-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        let executable = bin.appendingPathComponent("claude")
        try Data(agent.utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        return root
    }

    /// 写入 `$NORMA_BRAIN_DIR/global/current/proposals/{sha256(scope)}.json`。
    /// scope 是桌面会话身份 `desktop:{workspace}:{area}`，不是 workspace_identities 的 id。
    @discardableResult
    public static func writeProposals(
        brain: URL,
        scope: String,
        agent: String,
        taskMarker: String
    ) throws -> URL {
        let digest = SHA256.hash(data: Data(scope.utf8)).map { String(format: "%02x", $0) }.joined()
        let directory = brain.appendingPathComponent("global/current/proposals", isDirectory: true)
        guard FileManager.default.fileExists(atPath: directory.path) else {
            throw E2EError.product("大脑当前代没有 proposals 目录：\(directory.path)")
        }
        let body: [String: Any] = [
            "next_id": 2,
            "workspace_id": scope,
            "items": [
                item(1, scope, agent, "整理日志", "\(taskMarker) 把今天的日志收成三条", "日志散落，先收成一页"),
                item(2, scope, "Nobody", "未知目标", "ISLAND-PROPOSAL-BLOCKED", "这个名字不在名册里"),
            ],
        ]
        let data = try JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys])
        let url = directory.appendingPathComponent("\(digest).json")
        try data.write(to: url, options: .atomic)
        return url
    }

    public static func titles(_ file: URL) throws -> [String] {
        let data = try Data(contentsOf: file)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let items = object["items"] as? [[String: Any]] else {
            throw E2EError.product("提议文件不是预期的对象：\(file.path)")
        }
        return items.compactMap { $0["title"] as? String }
    }

    private static func item(
        _ id: Int,
        _ scope: String,
        _ agent: String,
        _ title: String,
        _ task: String,
        _ why: String
    ) -> [String: Any] {
        [
            "id": id,
            "workspace_id": scope,
            "agent": agent,
            "title": title,
            "task": task,
            "why": why,
            "at": 1_700_000_000 + id,
        ]
    }

    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, shlex, subprocess, sys, time
    settings = json.loads(sys.argv[sys.argv.index('--settings') + 1])
    forward = settings['hooks']['SessionStart'][0]['hooks'][0]['command']
    sid = 'island-proposals-' + os.environ.get('NORMA_CONTROL_RUN_ID', 'local')
    cwd = os.getcwd()
    def hook(name, raw):
        raw = dict(raw, hook_event_name=name, session_id=sid, cwd=cwd, prompt_id='island-proposal-turn')
        result = subprocess.run(shlex.split(forward), input=json.dumps(raw), capture_output=True, text=True, timeout=10)
        if result.returncode: raise RuntimeError(result.stderr or result.stdout)
    hook('SessionStart', {'source':'startup'})
    hook('UserPromptSubmit', {'prompt':'Proposal agent stays idle'})
    hook('Stop', {})
    print('PROPOSAL AGENT READY', flush=True)
    while True: time.sleep(1)
    """#
}
