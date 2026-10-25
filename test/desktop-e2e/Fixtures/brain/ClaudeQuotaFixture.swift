import Foundation

/// 无网络额度夹具：先回放安装版 Claude adapter 的 consumer，随后用其 ACP 帧驱动真实 App。
public enum ClaudeQuotaFixture {
    public static func install() throws -> URL {
        let fm = FileManager.default
        let root = URL(fileURLWithPath: "/private/tmp/norma-claude-quota-\(UUID().uuidString)")
        let bin = root.appendingPathComponent("adapters/node_modules/.bin")
        try fm.createDirectory(at: bin, withIntermediateDirectories: true)
        let replay = Process(), output = Pipe()
        replay.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        replay.arguments = ["node", "crates/brain/tests/support/claude_quota_replay.mjs"]
        replay.standardOutput = output
        try replay.run()
        let frames = output.fileHandleForReading.readDataToEndOfFile()
        replay.waitUntilExit()
        guard replay.terminationStatus == 0 else { throw BrainFixtureError.missingAdapter("Claude consumer quota replay failed") }
        try frames.write(to: root.appendingPathComponent("adapter-output.json"))
        try #"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"summary":"off"}"#
            .write(to: root.appendingPathComponent("config.json"), atomically: true, encoding: .utf8)
        let adapter = bin.appendingPathComponent("claude-agent-acp")
        try script.write(to: adapter, atomically: true, encoding: .utf8)
        try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: adapter.path)
        return root
    }

    public static func installAgents() throws -> URL {
        let root = try BrainFixture.installFakeAgents()
        let script = root.appendingPathComponent("bin/claude")
        var text = try String(contentsOf: script, encoding: .utf8)
        text = text.replacingOccurrences(of: "def post(event, raw):", with: "round_no = 1\ndef post(event, raw):")
        text = text.replacingOccurrences(of: "\"brain-e2e-turn\"", with: "\"quota-hook-\" + str(round_no)")
        // Replace only the final idle, retaining the original first completion and real hook path.
        guard let range = text.range(of: "time.sleep(3600)", options: .backwards) else {
            throw BrainFixtureError.missingAdapter("fake Agent script shape changed")
        }
        text.replaceSubrange(range, with: """
        for round_no in [2, 3]:
            while not os.path.exists(os.path.join(os.environ['NORMA_BRAIN_DIR'], 'trigger-' + str(round_no))):
                time.sleep(0.05)
            post('UserPromptSubmit', {'prompt': 'quota controlled round ' + str(round_no)})
            time.sleep(0.3)
            post('Stop', {'last_assistant_message': '需要你核对 quota controlled round ' + str(round_no)})
        open(os.path.join(os.environ['NORMA_BRAIN_DIR'], 'hooks-complete'), 'w').write('done')
        time.sleep(3600)
        """)
        text = text.replacingOccurrences(of: "time.sleep(2)\npost(\"UserPromptSubmit\"", with: "if os.path.exists(os.path.join(os.environ['NORMA_BRAIN_DIR'], 'hooks-complete')):\n    time.sleep(3600)\ntime.sleep(2)\npost(\"UserPromptSubmit\"")
        try text.write(to: script, atomically: true, encoding: .utf8)
        return root
    }

    /// Read-only SQLite dump; no activity rows or lifecycle state are seeded by the fixture.
    public static func rows(_ database: URL, sql: String) throws -> [[String: Any]] {
        let p = Process(), out = Pipe()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
        p.arguments = ["-json", database.path, sql]
        p.standardOutput = out
        try p.run()
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        guard p.terminationStatus == 0 else { throw BrainFixtureError.missingAdapter("SQLite read failed") }
        return data.isEmpty ? [] : (try JSONSerialization.jsonObject(with: data) as? [[String: Any]] ?? [])
    }

    private static let script = #"""
    #!/usr/bin/env python3
    import json, os, pathlib, sys, time
    root = pathlib.Path(os.environ['NORMA_BRAIN_DIR'])
    sid = 'claude-quota-replay-' + str(os.getpid())
    frames = json.loads((root / 'adapter-output.json').read_text())
    def log(value):
        with (root / 'replay.jsonl').open('a') as f: f.write(json.dumps(value) + '\n')
    def marker(name):
        temp = root / (name + '.tmp')
        temp.write_text(sid)
        temp.replace(root / name)
    def send(value):
        log({'direction':'out','session':sid,'frame':value})
        print(json.dumps(value), flush=True)
    for line in sys.stdin:
        m = json.loads(line); method = m.get('method'); mid = m.get('id')
        log({'direction':'in','session':sid,'frame':m})
        if method == 'initialize': send({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
        elif method == 'session/new': send({'jsonrpc':'2.0','id':mid,'result':{'sessionId':sid}})
        elif method == 'session/prompt':
            count_path = root / 'prompt-count'
            count = int(count_path.read_text()) + 1 if count_path.exists() else 1
            count_path.write_text(str(count))
            for original in frames:
                frame = json.loads(json.dumps(original))
                if 'params' in frame: frame['params']['sessionId'] = sid
                if 'id' in frame: frame['id'] = mid
                if count == 2 and 'error' in frame:
                    # Pending provider result is deliberately NOT emitted before cancellation.
                    # A different update flushes the already-received quota text immediately.
                    send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'fixture_waiting_for_restart'}}})
                    marker('pending-ready')
                    time.sleep(120)
                if 'error' in frame: marker('completed-' + str(count))
                send(frame)
        elif mid is not None: send({'jsonrpc':'2.0','id':mid,'result':{}})
    """#
}
