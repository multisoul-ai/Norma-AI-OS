import E2EKit
import Fixtures
import Foundation

/// 后台大脑：Agent 完成后，真实的 ACP 适配器（Claude Code）收到事件，
/// 通过 `norma brain report` 把汇报文字交回 App，并在幕僚活动里核对正文。
/// 默认没有 TTS；brain-report-voice 只在隔离数据库选择本机已有的本地 TTS。
@MainActor
public enum BrainReport {
    public nonisolated static let name = "brain-report"
    public nonisolated static let voiceName = "brain-report-voice"

    public static func run(_ session: Session, speech: Bool = false) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            // 失败时也留下大脑的记录和适配器日志，方便排查。
            if let brainDir, let output = session.output {
                for name in ["decisions.jsonl", "approvals.jsonl", "adapter.log"] {
                    try? FileManager.default.copyItem(
                        at: BrainSupport.stateFiles(brainDir, named: name).first ?? brainDir.appendingPathComponent(name),
                        to: output.appendingPathComponent("brain-\(name)"))
                }
            }
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let workspace = "BrainCheck"

        try await session.step("建立含项目区的隔离工作空间，开启大脑") {
            let cli = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("norma")
            guard FileManager.default.isExecutableFile(atPath: cli.path) else {
                throw E2EError.env("缺少配套 norma CLI；请先 cargo build -p norma，再运行 brain-report")
            }
            if speech {
                let model = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Norma OS/models/tts/matcha-icefall-zh-en/installation.json")
                guard FileManager.default.fileExists(atPath: model.path) else {
                    throw E2EError.env("实际播音需要本机已安装 Matcha Chinese-English；请先在语音设置安装该本地模型")
                }
            }
            fakeShell = try BrainFixture.installFakeAgents(finalMessage: speech
                ? "登录页完成，需要你确认旧接口是否保留。"
                : "登录页已经做完，12 个测试全部通过，但需要你确认是否保留旧的登录接口。")
            brainDir = try BrainFixture.installBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_BRAIN_ADAPTER_LOG", brainDir!.appendingPathComponent("adapter.log").path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            if speech {
                // 只写已停止的测试数据库；不复制凭据，不改用户数据库或系统输出设备。
                try SQLRaw.execute(database: session.databaseURL(), statements: [
                    ("voice.desktop.mode", "legacy"), ("voice.tts.active_provider_id", "local"),
                    ("voice.tts.model_id", "matcha-icefall-zh-en"), ("voice.tts.voice_id", "default"),
                    ("voice.agent_task_announcements_enabled", "false"),
                ].map { (sql: "INSERT INTO settings(key,value,updated_at_unix) VALUES(?1,?2,0) ON CONFLICT(key) DO UPDATE SET value=excluded.value", bindings: [$0.0, $0.1]) })
            }
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目，创建一个会自己完成任务的 Claude 智能体") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(
                    local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540),
                    on: launch
                )
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            // 功能引导会在进入项目后弹出并盖住画面；文字识别可能把标题认错，所以同时认“跳过引导”。
            try await BrainSupport.dismissGuide(session)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            try await BrainSupport.dismissGuide(session)
            // 命令可能被刚弹出的引导吞掉：最多重试三次，直到画布上出现智能体卡片。
            var created = false
            for _ in 0..<3 {
                let frame = try await session.capture()
                try await session.click(
                    local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                    on: frame
                )
                try await session.selectAll()
                try await session.type("/new claude")
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 2_500_000_000)
                let after = try await session.capture()
                if after.contains("运行中") || after.contains("待开始") { created = true; break }
                try await BrainSupport.dismissGuide(session)
            }
            guard created else { throw E2EError.product("三次尝试后画布上仍没有智能体卡片") }
            return try await session.capture().summary
        }

        try await session.step(speech ? "真实大脑汇报入队，并通过 macOS 音频设备播完" : "真实大脑交回汇报；无 TTS 时明确跳过语音，不能记为已入队") {
            let log = try appLog(session)
            try await waitForLog(log, containing: "Norma 大脑已启动", timeout: 20)
            // Agent 约 4 秒后 Stop；大脑用真实 Claude Code 处理，首次约 10 到 40 秒。
            try await waitForLog(log, containing: speech ? "大脑播报已入队" : "大脑播报被跳过：TTS 不可用", timeout: 150)
            if speech {
                // 45s = 本地冷启动合成预算 30s + 短句播放 10s + 调度余量 5s。
                try await waitForLog(log, containing: "agent announcement playback started node_id=0 outcome=Report", timeout: 45)
                try await waitForLog(log, containing: "agent announcement playback finished node_id=0 outcome=Report completed=true", timeout: 45)
            }
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            // 日志里出现汇报时，这一轮可能还没结束；等主会话把回复写进决策记录。
            var text = ""
            let deadline = Date().addingTimeInterval(90)
            while Date() < deadline {
                let files = BrainSupport.stateFiles(brainDir, named: "decisions.jsonl")
                text = files.compactMap { try? String(contentsOf: $0, encoding: .utf8) }.joined(separator: "\n")
                if text.contains("Main") { break }
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            guard text.contains("Main") else {
                throw E2EError.product("画布状态目录的决策日志里没有主会话的回复")
            }
            let database = try session.databaseURL()
            let finished = Date().addingTimeInterval(10)
            while try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM brain_turns WHERE lane='Main' AND stop_reason='end_turn'") != 1 {
                guard Date() < finished else { throw E2EError.product("主会话没有持久化成功结束") }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            let allowed = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM brain_trace_items WHERE kind='permission' AND title='权限请求：放行' AND instr(text,' brain report --text ')>0") ?? 0
            let queued = speech ? "true" : "false"
            let received = try SQLRaw.scalar(database: database, #"SELECT COUNT(*) FROM brain_trace_items WHERE kind='tool_result' AND title LIKE '% completed' AND instr(text,'"ok":true')>0 AND instr(replace(replace(text,'\',''),' ',''),'"queued":\#(queued)')>0"#) ?? 0
            guard allowed > 0, received > 0 else { throw E2EError.product("缺少合法 report 的成功回执 queued=\(queued)") }
            let errors = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM brain_trace_items WHERE kind='error'") ?? 0
            guard errors == 0 else { throw E2EError.product("真实模型轮次出现错误，不能记为成功") }
            return speech ? "queued=true；真实本地 TTS 合成并收到 macOS DataPlayedBack 完成回调" : "真实 report 经权限闸门送达 App，queued=false；模型正常结束，无 TTS、未播音"
        }

        try await session.step("打开真实幕僚活动，展开汇报正文与 App 回执") {
            let panel = try await BrainActivity.openPanel(session)
            guard let turn = panel.pick(["第1轮", "第 1 轮", "主会话"], .top) else {
                throw E2EError.product("面板中没有真实主会话：\(panel.summary)")
            }
            try await session.click(local: turn.center, on: panel)
            let identity = try await session.wait(Expectation(allOf: ["会话关联"], timeout: 10))
            try session.save(identity, "report-identity.png")
            var commandVisible = false, receiptVisible = false
            for _ in 0..<30 {
                let frame = try await session.capture()
                if !commandVisible && frame.contains("report --text") && frame.contains("接口") && frame.contains("放行") {
                    try session.save(frame, "report-delivered.png")
                    commandVisible = true
                }
                if !receiptVisible && frame.contains("queued") && frame.contains(speech ? "true" : "false") {
                    try session.save(frame, "report-receipt.png")
                    receiptVisible = true
                }
                if commandVisible && receiptVisible { break }
                try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -200)
            }
            guard commandVisible, receiptVisible else {
                throw E2EError.product("真实面板缺少汇报正文或回执：command=\(commandVisible), receipt=\(receiptVisible)")
            }
            return "幕僚活动中可见汇报正文、权限放行及 queued=\(speech) 回执"
        }
    }

    private static func appLog(_ session: Session) throws -> URL {
        guard let output = session.output else { throw E2EError.env("运行目录丢失") }
        return output.appendingPathComponent("app.log")
    }

    private static func waitForLog(_ url: URL, containing needle: String, timeout: TimeInterval) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let text = try? String(contentsOf: url, encoding: .utf8), text.contains(needle) { return }
            try await Task.sleep(nanoseconds: 500_000_000)
        }
        throw E2EError.product("应用日志里 \(Int(timeout)) 秒内没有出现「\(needle)」：\(url.path)")
    }
}
