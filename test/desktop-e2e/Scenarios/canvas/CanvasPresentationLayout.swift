import E2EKit
import Fixtures
import Foundation

/// 固定窗口、多 Agent/终端下，经生产 CLI 反复 Present；截图和正文位置核对可见布局。
@MainActor
public enum CanvasPresentationLayout {
    public nonisolated static let name = "canvas-presentation-layout"

    public static func run(_ session: Session) async throws {
        let workspace = "PresentationLayoutCheck"
        var root: URL?
        let fake = try CanvasPresentationFixture.install(cli: URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("norma").path)
        let web = try ContentBrowserFixture()
        let oldZdot = ProcessInfo.processInfo.environment["ZDOTDIR"]
        defer {
            session.stop()
            web.stop()
            if let oldZdot { setenv("ZDOTDIR", oldZdot, 1) } else { unsetenv("ZDOTDIR") }
            if let root { try? FileManager.default.removeItem(at: root) }
            try? FileManager.default.removeItem(at: fake)
        }
        try await session.step("建立固定尺寸的隔离项目") {
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            let db = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.suppressFeatureGuide(database: db)
            let project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: workspace)
            root = project
            try Data("# PRESENTATION FILE CONTENT\n".utf8).write(to: project.appendingPathComponent("Present.txt"))
            try CanvasPresentationFixture.authorize(database: db, homeDirectory: fake)
            setenv("ZDOTDIR", fake.path, 1)
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            try await session.resize(width: 1440, height: 840)
            try await openProject(session, workspace: workspace)
            return "固定 1440×840，使用隔离工作空间及假 Agent"
        }
        try await session.step("创建六张 Agent 和两张终端，记录展示前布局") {
            for _ in 0..<6 { try await command(session, "/new claude") }
            for index in 1...2 {
                try await command(session, "/terminal")
                try await startTerminalOutput(session, index: index)
            }
            let frame = try await healthyLayout(session)
            try session.save(frame, "01-before-present.png")
            return "六张 Agent 和两张终端正文可见，画布没有细条或隐藏空档"
        }
        try await session.step("合法可见终端 Master") {
            if session.visualReview {
                try await CanvasPresentationVisual.master(session)
                return "模型核对合法 Master 的主区和七张 detail，并验证退出后的八卡布局"
            }
            try await session.click(matching: ["TERMONE"], pick: .top)
            try await session.shortcut(key: 53, modifiers: [])
            try await session.shortcut(key: 46, modifiers: [0x37])
            let deadline = Date().addingTimeInterval(15)
            var verified = false
            var lastFrame: Frame?
            while Date() < deadline {
                let frame = try await session.capture()
                lastFrame = frame
                let agents = frame.texts.filter { $0.value.contains("AGENT") && !$0.value.contains("Agent") }
                let terms = terminalMarkers(frame, index: 1)
                if agents.count >= 6, agents.allSatisfy({ $0.center.x > 700 }),
                   terminalMarkers(frame, index: 2).contains(where: { $0.center.x > 700 }),
                   terms.contains(where: { $0.center.x < 450 }) {
                    try session.save(frame, "01-visible-master.png")
                    verified = true
                    break
                }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            guard verified else {
                if let lastFrame { try session.save(lastFrame, "failure-master.png") }
                throw E2EError.product("合法终端 Master 未占左侧主区域，或可见 detail 缺失")
            }
            try await session.shortcut(key: 46, modifiers: [0x37])
            _ = try await healthyLayout(session)
            return "可见终端可以进入主次布局并正常退出，六张 Agent 和另一终端 detail 保持可读"
        }
        guard let root, let node = try CanvasPresentationFixture.agents(fake).first else {
            throw E2EError.product("假 Agent 未启动，无法通过生产来源展示内容")
        }
        var browserID: UInt64?
        var previewID: UInt64?
        for iteration in 1...5 {
            try await session.step("第 \(iteration) 轮网页与显式 Markdown 预览") {
                let browser = try await present(fake, node: node, args: ["present", "browser", web.address])
                try requireReceipt(browser, kind: "browser")
                try requireReuse(browser, iteration: iteration, previousID: &browserID)
                try saveReceipt(session, browser, "browser-\(iteration)")
                let page = try await content(session, text: "BROWSER FIRST E2E", browser: true)
                if iteration == 1 { try session.save(page, "02-browser-content.png") }
                if !session.visualReview { _ = try await healthyLayout(session) }
                let preview = try await present(fake, node: node, args: ["present", "preview", root.appendingPathComponent("Present.txt").path, "--type", "markdown"])
                try requireReceipt(preview, kind: "preview")
                try requireReuse(preview, iteration: iteration, previousID: &previewID)
                try saveReceipt(session, preview, "preview-\(iteration)")
                let shown = try await content(session, text: "PRESENTATION FILE CONTENT", browser: false)
                if !session.visualReview { _ = try await healthyLayout(session) }
                try session.save(shown, "02-present-\(iteration).png")
                return "网页和文件回执均为 content_drawer；可见卡片正文和位置正常"
            }
        }
        try await session.step("独立副本与类型切换") {
            let path = root.appendingPathComponent("Present.txt").path
            let result = try await present(fake, node: node, args: ["present", "preview", path, "--type", "text", "--new"])
            try requireReceipt(result, kind: "preview")
            try saveReceipt(session, result, "preview-copy")
            guard result["disposition"] as? String == "created",
                  (result["node_id"] as? NSNumber)?.uint64Value != previewID else {
                throw E2EError.product("--new 未创建独立表面并返回 created")
            }
            let shown = try await content(session, text: session.visualReview ? "# PRESENTATION FILE CONTENT" : "PRESENTATION FILE CONTENT", browser: false)
            let tabs = try session.contentTabs(workspaceName: workspace)
            guard tabs.filter({ $0.kind == "file" }).count == 2 else { throw E2EError.product("--new 没有保留两个独立文件标签") }
            let frame = session.visualReview ? shown : try await healthyLayout(session)
            try session.save(frame, "03-independent-preview.png")
            return "--new 保留独立身份，--type text 可显示原始正文"
        }
        try await session.step("手动重排与重启恢复") {
            try await session.shortcut(key: 37, modifiers: [0x37, 0x38])
            let arranged = try await healthyLayout(session)
            try session.save(arranged, "04-manual-arrange.png")
            session.stop()
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            try await session.resize(width: 1440, height: 840)
            try await openProject(session, workspace: workspace)
            try await restartTerminalOutput(session)
            let restored = try await healthyLayout(session)
            try session.save(restored, "05-after-restart.png")
            return "手动重排及项目恢复后，所有可见卡片仍正常占位"
        }
        try await session.step("恢复旧隐藏 Master 项目") {
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            try CanvasPresentationFixture.seedLegacyMaster(dataRoot: data, project: root)
            try session.launch(language: "zh-Hans", homeDirectory: fake)
            try await session.resize(width: 1440, height: 840)
            try await openProject(session, workspace: workspace)
            try await restartTerminalOutput(session)
            let recovered = try await healthyLayout(session)
            try session.save(recovered, "06-legacy-master-recovered.png")
            return "旧隐藏网页 Master 被生产恢复路径修复；无细条、无首格空档"
        }
    }

    private static func openProject(_ session: Session, workspace: String) async throws {
        if session.visualReview {
            try await session.visualClick("确认工作空间 \(workspace) 的目录卡片可见，定位该卡片的打开按钮")
            try await session.visualClick("确认已进入工作空间，在项目区定位 E2E Project 的打开入口")
        } else {
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await session.click(matching: ["打开", "Open"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 15))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
        }
    }

    private static func content(_ session: Session, text: String, browser: Bool) async throws -> Frame {
        if session.visualReview {
            return try await CanvasPresentationVisual.layout(session, content: text, browser: browser)
        }
        return try await session.wait(Expectation(allOf: [text], timeout: 15))
    }

    private static func startTerminalOutput(_ session: Session, index: Int) async throws {
        // 持续输出让终端经历真实 resize 后仍有新正文可验，避免依赖一次输出的回卷位置。
        // 单词标记避免 Vision 把重复行分成纵向文字列；后缀经参数生成，命令回显不能匹配。
        let suffix = index == 1 ? "ONE" : "TWO"
        try await session.type("while :; do printf 'TERM%s\\n' \(suffix); sleep 1; done")
        try await session.shortcut(key: 36, modifiers: [])
    }

    private static func restartTerminalOutput(_ session: Session) async throws {
        if session.visualReview {
            for (index, side) in [(1, "左侧"), (2, "右侧")] {
                try await session.visualClick("确认项目已恢复六张 Agent 和两张终端；定位末行\(side)终端的正文输入区域，避开标题栏")
                try await startTerminalOutput(session, index: index)
            }
            return
        }
        // 生产恢复重建 fresh shell，不重放用户输入；真实点击终端正文后重新启动夹具输出。
        let deadline = Date().addingTimeInterval(15)
        var last = ""
        while Date() < deadline {
            let frame = try await session.capture()
            last = frame.summary
            let titles = terminalTitles(frame)
            if titles.count == 2 {
                for (index, title) in titles.enumerated() {
                    try await session.click(local: CGPoint(x: title.center.x + 40, y: title.center.y + 45), on: frame)
                    try await startTerminalOutput(session, index: index + 1)
                }
                return
            }
            try await Task.sleep(nanoseconds: 300_000_000)
        }
        throw E2EError.product("恢复后没有恰好两张终端卡片标题：\(last)")
    }

    private static func terminalTitles(_ frame: Frame) -> [VisibleText] {
        // OCR 可将同一标题栏的类型徽标和标题分开；按空间邻近合并，避免输入同一卡两次。
        let candidates = frame.texts.filter { $0.value.trimmingCharacters(in: .whitespaces).hasPrefix("终端") }
            .sorted { abs($0.center.y - $1.center.y) < 10 ? $0.center.x < $1.center.x : $0.center.y < $1.center.y }
        var titles: [VisibleText] = []
        for candidate in candidates {
            if !titles.contains(where: { abs($0.center.y - candidate.center.y) < 10 && abs($0.center.x - candidate.center.x) < 100 }) {
                titles.append(candidate)
            }
        }
        return titles
    }

    private static func terminalMarkers(_ frame: Frame, index: Int) -> [VisibleText] {
        let marker = index == 1 ? "TERMONE" : "TERMTWO"
        return frame.texts.filter { $0.value.trimmingCharacters(in: .whitespacesAndNewlines) == marker }
    }

    private static func command(_ session: Session, _ text: String) async throws {
        let frame = try await (session.visualReview ? session.capturePixels() : session.capture())
        try await session.click(local: CGPoint(x: frame.windowFrame.width / 2, y: frame.windowFrame.height - 38), on: frame)
        try await session.selectAll()
        try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 900_000_000)
    }

    private static func healthyLayout(_ session: Session) async throws -> Frame {
        if session.visualReview { return try await CanvasPresentationVisual.layout(session) }
        let deadline = Date().addingTimeInterval(15)
        var last = ""
        var lastFrame: Frame?
        while Date() < deadline {
            let frame = try await session.capture()
            last = frame.summary
            lastFrame = frame
            let markers = frame.texts.filter { $0.value.contains("AGENT") && !$0.value.contains("Agent") }
            // 固定窗口八卡平铺时两张终端在末行相邻列；相隔超过 150pt 排除同一卡的两行回显。
            let separateTerminals = terminalMarkers(frame, index: 1).contains { first in
                terminalMarkers(frame, index: 2).contains { abs($0.center.x - first.center.x) > 150 }
            }
            if markers.count >= 6, separateTerminals,
               (markers.map { $0.center.x }.min() ?? 9999) < 450,
               (markers.map { $0.center.y }.max() ?? 0) - (markers.map { $0.center.y }.min() ?? 0) > 200 {
                return frame
            }
            try await Task.sleep(nanoseconds: 300_000_000)
        }
        if let lastFrame { try session.save(lastFrame, "failure-layout.png") }
        throw E2EError.product("八张卡片正文未完整可见，或布局仍挤在右侧：\(last)")
    }

    private static func present(_ root: URL, node: String, args: [String]) async throws -> [String: Any] {
        let response = try CanvasPresentationFixture.request(root, node: node, args: args + ["--format", "json"])
        let deadline = Date().addingTimeInterval(30)
        while !FileManager.default.fileExists(atPath: response.path), Date() < deadline {
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        guard let data = try? Data(contentsOf: response),
              let envelope = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              envelope["code"] as? Int == 0,
              let stdout = envelope["stdout"] as? String,
              let reply = try JSONSerialization.jsonObject(with: Data(stdout.utf8)) as? [String: Any],
              let result = reply["result"] as? [String: Any] else {
            throw E2EError.product("生产 Present 失败：\((try? String(contentsOf: response, encoding: .utf8)) ?? "没有收到回执")")
        }
        return result
    }

    private static func saveReceipt(_ session: Session, _ result: [String: Any], _ name: String) throws {
        guard let output = session.output else { throw E2EError.env("缺少证据目录") }
        try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
            .write(to: output.appendingPathComponent("receipt-\(name).json"))
    }

    private static func requireReuse(_ result: [String: Any], iteration: Int, previousID: inout UInt64?) throws {
        let id = (result["node_id"] as? NSNumber)?.uint64Value
        let expected = iteration == 1 ? "created" : "reused"
        guard result["disposition"] as? String == expected,
              iteration == 1 || id == previousID else {
            throw E2EError.product("第 \(iteration) 轮未如实复用同一展示表面：\(result)")
        }
        previousID = id
    }

    private static func requireReceipt(_ result: [String: Any], kind: String) throws {
        guard result["layout"] as? String == "content_drawer", result["kind"] as? String == kind,
              (result["node_id"] as? NSNumber)?.uint64Value ?? 0 > 0 else {
            throw E2EError.product("展示回执不真实：\(result)")
        }
    }
}
