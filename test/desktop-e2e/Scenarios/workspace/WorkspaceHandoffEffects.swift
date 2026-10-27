import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// Agent 之间的交接（`/ask-norma` 背后的 `norma agent task assign`）在真实画布上的派发特效。
/// 构造：隔离工作空间 + 三张假 Agent 卡片；源卡片经生产 CLI 把无副作用任务派给另一张卡片。
/// 三种设置 × 浅/深主题：未自定义 → 流星；只设语音 → 交接跟随语音（烟花）；显式设交接 → 保留覆盖（闪电）。
/// 预期：动画有明显起止并落在目标卡片附近，结束后无残留；时长落在各款式区间（闪电 < 流星 < 烟花）。
@MainActor
public enum WorkspaceHandoffEffects {
    public nonisolated static let name = "workspace-handoff-effects"
    private static let workspace = "HandoffFX"

    private struct Branch {
        let tag: String
        let json: String?
        let style: String
        let span: ClosedRange<Int>
    }

    // 区间按“可见区域内的活动跨度”实测标定（约 180ms 取样粒度）：流星约 0.92s、烟花约 1.5–1.7s、闪电约 0.2–0.4s。
    private static let branches = [
        Branch(tag: "default", json: nil, style: "meteor", span: 600...1250),
        Branch(tag: "voice-only", json: "{\"style\":\"meteor\",\"voice\":\"fireworks\"}", style: "fireworks", span: 1300...2300),
        Branch(tag: "handoff-override", json: "{\"style\":\"meteor\",\"voice\":\"fireworks\",\"handoff\":\"lightning\"}", style: "lightning", span: 0...520),
    ]
    // 浅色组配最亮的内置壁纸，验证特效在亮背景上仍清晰；深色组保留默认太空壁纸。
    private static let themes = [
        ("light", "paper-blue", "wallpapers/norma-silver-moonscape.png"),
        ("dark", "midnight-violet", "wallpapers/desktop-earth.png"),
    ]

    public static func run(_ session: Session) async throws {
        let cli = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("norma").path
        let fake = try CanvasPresentationFixture.install(cli: cli)
        try makeSourceRunnable(fake)
        let oldZdot = ProcessInfo.processInfo.environment["ZDOTDIR"]
        var root: URL?
        defer {
            session.stop()
            if let oldZdot { setenv("ZDOTDIR", oldZdot, 1) } else { unsetenv("ZDOTDIR") }
            if let root { try? FileManager.default.removeItem(at: root) }
            try? FileManager.default.removeItem(at: fake)
        }

        try await session.step("建立隔离工作空间、项目与三张假 Agent") {
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            let db = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.suppressFeatureGuide(database: db)
            root = try CanvasProjectFixture.install(dataRoot: data, workspaceName: workspace)
            try CanvasPresentationFixture.authorize(database: db, homeDirectory: fake)
            setenv("ZDOTDIR", fake.path, 1)
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            try await session.resize(width: 1440, height: 840)
            try await openProject(session)
            for _ in 0..<3 { try await command(session, "/new claude") }
            _ = try await markers(session, count: 3)
            return "三张假 Agent 已启动"
        }

        for (themeName, themeID, wallpaper) in themes {
            for branch in branches {
                try await session.step("\(themeName) 主题 · \(branch.tag)：交接应为\(branch.style)") {
                    session.stop()
                    // 上一轮的“运行中”标记不能带到新进程：Hook 端点未就绪时假 Agent 会提前退出。
                    for file in (try? FileManager.default.contentsOfDirectory(atPath: fake.path)) ?? [] where file.hasPrefix("running-") {
                        try? FileManager.default.removeItem(at: fake.appendingPathComponent(file))
                    }
                    let db = try session.databaseURL()
                    try Catalog.seedSetting(database: db, key: "appearance.theme.v1", value: themeID)
                    // 迁移后壁纸按工作空间存取，旧的 wallpaper.* 设置行不再生效。
                    try SQLRaw.execute(database: db, statements: [(
                        sql: "UPDATE desktop_workspaces SET wallpaper_json=?1 WHERE name=?2",
                        bindings: ["{\"kind\":\"builtin\",\"rel\":\"\(wallpaper)\"}", workspace]
                    )])
                    try Catalog.seedWorkspaceDispatchEffect(database: db, workspaceName: workspace, json: branch.json ?? "") // 空串解码失败，等同未设置
                    try session.launch(language: "zh-Hans", homeDirectory: fake)
                    try await session.resize(width: 1440, height: 840)
                    try await openProject(session)
                    let cards = try await markers(session, count: 3)
                    let span = try await handoff(session, fake: fake, cards: cards, tag: "\(themeName)-\(branch.tag)", checkOrigin: branch.style != "lightning")
                    guard branch.span.contains(span) else {
                        throw E2EError.product("\(branch.tag)/\(themeName)：期望\(branch.style)时长 \(branch.span)ms，实测 \(span)ms")
                    }
                    return "\(branch.style) 动画跨度 \(span)ms，落点在目标卡片，收尾无残留"
                }
            }
        }
    }

    /// 真实 `/ask-norma` 发生在源 Agent 的运行回合里；让标记了 `running-<node>` 的假 Agent 经真实 Hook 上报回合开始。
    private static func makeSourceRunnable(_ root: URL) throws {
        let hook = """
        def post_running():
            raw = {"hook_event_name": "UserPromptSubmit", "session_id": "e2e-session", "prompt_id": "e2e-turn"}
            body = {"node_id": int(node), "profile": "Claude", "provider_event": "UserPromptSubmit",
                    "session_id": "e2e-session", "turn_id": "e2e-turn", "tool_name": None, "message": None, "raw_json": raw}
            import urllib.request
            request = urllib.request.Request(os.environ["MULTISOUL_HOOK_ENDPOINT"], json.dumps(body).encode(),
                {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"], "Content-Type": "application/json"})
            urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
        posted = False
        while True:
            if not posted and (root / ("running-" + node)).exists():
                try:
                    post_running(); posted = True
                except Exception:
                    pass
        """
        for name in ["claude", "codex"] {
            let url = root.appendingPathComponent("bin/\(name)")
            var text = try String(contentsOf: url, encoding: .utf8)
            text = text.replacingOccurrences(of: "while True:\n", with: hook + "\n")
            try Data(text.utf8).write(to: url)
        }
    }

    private static func openProject(_ session: Session) async throws {
        _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
        // 重启后 App 会直接恢复到上次打开的项目画布。
        for _ in 0..<35 {
            try await Task.sleep(nanoseconds: 700_000_000)
            let frame = try await session.capture()
            if frame.contains("LAYOUT") || frame.contains("Alan") { return }
        }
        try await session.click(matching: ["打开", "Open"], pick: .bottom)
        _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 15))
        try await session.click(matching: ["E2E Project"], pick: .bottom)
        try await Task.sleep(nanoseconds: 1_500_000_000)
    }

    private static func command(_ session: Session, _ text: String) async throws {
        let frame = try await session.capture()
        try await session.click(local: CGPoint(x: frame.windowFrame.width / 2, y: frame.windowFrame.height - 38), on: frame)
        try await session.selectAll()
        try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 1_200_000_000)
    }

    /// 假 Agent 启动后在卡片里打印 `LAYOUT AGENT <node>`；按它定位每张卡片。
    private static func markers(_ session: Session, count: Int) async throws -> [(node: String, center: CGPoint)] {
        let deadline = Date().addingTimeInterval(25)
        var last = ""
        while Date() < deadline {
            let frame = try await session.capture()
            last = frame.summary
            var found: [String: CGPoint] = [:]
            for text in frame.texts {
                guard let range = text.value.range(of: #"AGENT\s*(\d+)"#, options: .regularExpression) else { continue }
                let digits = text.value[range].filter(\.isNumber)
                if !digits.isEmpty { found[String(digits)] = text.center }
            }
            if found.count >= count {
                return found.map { (node: $0.key, center: $0.value) }.sorted { $0.center.x == $1.center.x ? $0.center.y < $1.center.y : $0.center.x < $1.center.x }
            }
            try await Task.sleep(nanoseconds: 400_000_000)
        }
        throw E2EError.product("没有定位到 \(count) 张假 Agent 卡片：\(last)")
    }

    private static func request(_ fake: URL, node: String, args: [String]) async throws -> [String: Any] {
        let response = try CanvasPresentationFixture.request(fake, node: node, args: args + ["--format", "json"])
        let deadline = Date().addingTimeInterval(30)
        while !FileManager.default.fileExists(atPath: response.path), Date() < deadline {
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        guard let data = try? Data(contentsOf: response),
              let envelope = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              envelope["code"] as? Int == 0,
              let stdout = envelope["stdout"] as? String,
              let reply = try JSONSerialization.jsonObject(with: Data(stdout.utf8)) as? [String: Any] else {
            throw E2EError.product("生产 CLI 调用失败：\((try? String(contentsOf: response, encoding: .utf8)) ?? "没有回执")")
        }
        return reply
    }

    /// 以第一张卡片为源、最后一张为目标，经生产 CLI 派发，并连续取样判断动画。
    private static func handoff(_ session: Session, fake: URL, cards: [(node: String, center: CGPoint)], tag: String, checkOrigin: Bool) async throws -> Int {
        let source = cards[0], target = cards[cards.count - 1], bystander = cards[1]
        let listing = try await request(fake, node: source.node, args: ["agent", "list"])
        guard let result = listing["result"] as? [String: Any],
              let agents = result["agents"] as? [[String: Any]],
              let targetName = agents.first(where: { ($0["active_node_id"] as? NSNumber)?.stringValue == target.node })?["name"] as? String else {
            throw E2EError.product("agent list 里找不到目标卡片 \(target.node)：\(listing)")
        }
        func rect(_ card: (node: String, center: CGPoint)) -> CGRect {
            // 标记文字在卡片左上角：卡片约 460×670pt。
            // 只取正文下方（y≥200pt）：目标卡片顶部会回显任务文字，不属于特效。
            CGRect(x: card.center.x - 70, y: 200, width: 460, height: 460)
        }
        FileManager.default.createFile(atPath: fake.appendingPathComponent("running-\(source.node)").path, contents: Data())
        try await Task.sleep(nanoseconds: 1_500_000_000)
        let size = try await session.capture().windowFrame.size
        // 源卡片处于“运行中”，边框有持续动画，不能算特效残留：活动区域取源卡片右侧。
        // 只看源/目标卡片之间、正文文字下方的区域：避开卡片边框动画、选中描边与光标闪烁，飞行走廊与撞击点都在其中。
        let left = rect(source).maxX + 20
        let canvas = CGRect(x: left, y: 200, width: rect(target).maxX - 20 - left, height: 460)
        let baseline = try await session.snapshot()
        var noise = 0
        for _ in 0..<5 {
            try await Task.sleep(nanoseconds: 200_000_000)
            noise = max(noise, changedPixels(baseline, try await session.snapshot(), in: canvas, windowSize: size))
        }
        let response = try CanvasPresentationFixture.request(
            fake, node: source.node,
            args: ["agent", "task", "assign", targetName, "--task", "e2e handoff effect check, no action needed", "--format", "json"]
        )
        var samples: [CGImage] = []
        var times: [Int] = []
        let started = ProcessInfo.processInfo.systemUptime
        for _ in 0..<30 {
            samples.append(try await session.snapshot())
            times.append(Int((ProcessInfo.processInfo.systemUptime - started) * 1000))
            try await Task.sleep(nanoseconds: 60_000_000)
        }
        guard let data = try? Data(contentsOf: response),
              let envelope = try JSONSerialization.jsonObject(with: data) as? [String: Any], envelope["code"] as? Int == 0 else {
            throw E2EError.product("task assign 没有成功：\((try? String(contentsOf: response, encoding: .utf8)) ?? "无回执")")
        }
        guard let settled = samples.last else { throw E2EError.env("没有取得派发动画画面") }
        let changes = samples.map { changedPixels(baseline, $0, in: canvas, windowSize: size) }
        let active = changes.enumerated().filter { $0.element > noise + 150 }.map { times[$0.offset] }
        for (index, image) in samples.enumerated() where changes[index] > noise + 150 || index == samples.count - 1 { try session.saveSnapshot(image, "\(tag)-\(index).png") }
        try session.saveSnapshot(baseline, "\(tag)-baseline.png")
        if let output = session.output {
            let timeline: [String: Any] = ["times_ms": times, "changed_pixels": changes, "noise": noise, "source": source.node, "target": target.node]
            try JSONSerialization.data(withJSONObject: timeline, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent("\(tag)-timeline.json"))
        }
        // 闪电只有约 0.2s，约 180ms 的取样可能只抓到一帧，所以只要求至少一帧有动画。
        guard let first = active.first, let last = active.last else {
            throw E2EError.product("\(tag)：派发后画布没有动画（噪声 \(noise)，各取样 \(changes)）")
        }
        let tail = changedPixels(baseline, settled, in: canvas, windowSize: size)
        guard tail <= noise + 150 else { throw E2EError.product("\(tag)：取样结束后画布仍在变化（\(tail)）") }
        // 落点：目标卡片正文区必须出现特效；流星/烟花收尾帧的变化应集中在目标而不是旁观卡片（闪电的长电弧会横穿旁观卡片，只看绝对量）。
        let targetPeak = samples.map { changedPixels(baseline, $0, in: rect(target), windowSize: size) }.max() ?? 0
        guard targetPeak > noise + 150 else {
            throw E2EError.product("\(tag)：目标卡片上没有特效（峰值 \(targetPeak)）")
        }
        if checkOrigin, let lastIndex = changes.lastIndex(where: { $0 > noise + 150 }) {
            let atTarget = changedPixels(baseline, samples[lastIndex], in: rect(target), windowSize: size)
            let atBystander = changedPixels(baseline, samples[lastIndex], in: rect(bystander), windowSize: size)
            guard atTarget > atBystander else {
                throw E2EError.product("\(tag)：收尾帧的动画不在目标卡片（目标 \(atTarget)，旁观 \(atBystander)）")
            }
        }
        if checkOrigin {
            // 从源卡片出发的特效必须经过源卡片右侧到中间卡片的走廊；火箭等细小款式单帧可能很淡，所以看整段取样的峰值。
            let nearSource = CGRect(x: rect(source).maxX, y: 200, width: rect(bystander).midX - rect(source).maxX, height: 460)
            let corridorPeak = samples.map { changedPixels(baseline, $0, in: nearSource, windowSize: size) }.max() ?? 0
            guard corridorPeak > noise + 30 else {
                throw E2EError.product("\(tag)：动画没有经过源卡片一侧的走廊（峰值 \(corridorPeak)）")
            }
        }
        // 结束后与派发前基线比较：飞行走廊（卡片下方）不留残余。
        try requireAnimationCleared(reference: baseline, final: settled, in: canvas, windowSize: size, tolerance: noise + 150)
        return last - first
    }
}
