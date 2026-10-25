import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 自主动作：脚本化的大脑在第一个智能体完成后给第二个智能体派后续任务，
/// 幕僚活动面板里出现这条记录（左抽屉只有入口），展开后“以后这类先问我”和“来自第 N 轮”可用。
@MainActor
public enum BrainAutonomy {
    public nonisolated static let name = "brain-autonomy"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let workspace = "Autonomy"

        try await session.step("建立工作空间与项目，开启脚本化大脑") {
            fakeShell = try BrainFixture.installFakeAgents()
            brainDir = try BrainFixture.installScriptedBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目，创建两个智能体（Alan 会完成任务，Bella 待命）") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(
                    local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await BrainSupport.dismissGuide(session)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await BrainSupport.dismissGuide(session)
            for _ in 0..<2 {
                let frame = try await session.capture()
                try await session.click(
                    local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                    on: frame)
                try await session.selectAll()
                try await session.type("/new claude")
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 2_500_000_000)
                try await BrainSupport.dismissGuide(session)
            }
            return try await session.capture().summary
        }

        try await session.step("⌘⇧E 打开概览：左抽屉没有“Norma 自主处理”栏，只有“幕僚活动”入口") {
            try await BrainSupport.openOverview(session)
            let frame = try await session.wait(Expectation(allOf: ["运行概览", "幕僚活动"], timeout: 15))
            try session.save(frame, "01-drawer-entry.png")
            guard !frame.contains("Norma 自主处理") else {
                throw E2EError.product("概览里不该再有“Norma 自主处理”栏：\(frame.summary)")
            }
            return "入口在，旧栏已移除"
        }

        try await session.step("点抽屉入口打开幕僚活动，等派发记录出现，并且动作被标为已读") {
            var frame = try await session.capture()
            var presses = 0
            let deadline = Date().addingTimeInterval(150)
            while Date() < deadline {
                // 点左抽屉里的“幕僚活动”入口；点击偶尔没反应，面板没出现就再点，最多四次。
                if !frame.contains("保留最近"), presses < 4 {
                    try await session.click(matching: ["每一轮与自主处理"], pick: .bottom)
                    presses += 1
                }
                try await Task.sleep(nanoseconds: 2_000_000_000)
                frame = try await session.capture()
                // 小号中文偶尔识别出错字（“登录页”认成“堂家贝”），只认开头的“补上”。
                if frame.contains("派发") && frame.contains("补上") { break }
            }
            try session.save(frame, "02-autonomy-panel.png")
            for needle in ["幕僚活动", "保留最近", "派发", "补上", "触发"] {
                guard frame.contains(needle) else {
                    throw E2EError.product("面板里缺少「\(needle)」：\(frame.summary)")
                }
            }
            let database = try session.databaseURL()
            let deadlineSeen = Date().addingTimeInterval(8)
            while Date() < deadlineSeen {
                let unseen = try SQLRaw.scalar(
                    database: database,
                    "SELECT COUNT(*) FROM brain_actions WHERE at_ms > COALESCE((SELECT value FROM brain_activity_state WHERE key='actions_seen_at_ms'),0)")
                if unseen == 0 { return "面板出现派发记录，未读为 0" }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            throw E2EError.product("面板打开后动作仍有未读")
        }

        try await session.step("点这条动作展开，看到大脑给的理由、两个管理按钮和“来自第 N 轮”") {
            try await session.click(matching: ["补上"], pick: .top)
            let frame = try await session.wait(Expectation(allOf: ["理由", "来自第"], timeout: 8))
            try session.save(frame, "03-autonomy-expanded.png")
            return "展开了理由"
        }

        try await session.step("点“以后这类先问我”，配置关闭自行派任务") {
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let config = brainDir.appendingPathComponent("config.json")
            try await session.click(matching: ["以后这类先问我", "以后这类"], pick: .bottom)
            try session.save(try await session.capture(), "04-after-ask.png")
            let deadline = Date().addingTimeInterval(6)
            while Date() < deadline {
                if let text = try? String(contentsOf: config, encoding: .utf8),
                   text.contains("\"assign_followups\": false") {
                    return text
                }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            let text = (try? String(contentsOf: config, encoding: .utf8)) ?? ""
            throw E2EError.product("配置里没有出现 assign_followups=false：\(text)")
        }

        try await session.step("点“来自第 N 轮”：跳到大脑那一轮并展开，看到它执行的命令") {
            try await session.click(matching: ["来自第"], pick: .top)
            // 展开的一轮顶在列表最上面：先是“发给大脑”的提示全文；“复制本轮”在长提示之后，不在首屏。
            let expanded = try await session.wait(Expectation(allOf: ["发给大脑", "产生"], timeout: 8))
            try session.save(expanded, "05-source-turn.png")
            return "跳到来源轮并展开"
        }

        try await session.step("完成提示里带着用户交给 Agent 的任务（目标），且不是大脑自己派的 Bella 任务") {
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let log = brainDir.appendingPathComponent("prompt.log")
            let text = (try? String(contentsOf: log, encoding: .utf8)) ?? ""
            guard text.contains("用户最近交给各 Agent 的任务"), text.contains("实现登录页并补测试") else {
                throw E2EError.product("发给大脑的提示里没有目标：\(text.prefix(600))")
            }
            let goalPart = text.components(separatedBy: "用户最近交给各 Agent 的任务").last ?? ""
            guard !goalPart.contains("补上登录页的测试") else {
                throw E2EError.product("大脑自己派的任务被当成了目标：\(goalPart.prefix(400))")
            }
            return "目标进入了提示"
        }

        try await session.step("大脑用 history 读 Alan 的对话：假 Agent 没有手机控制，应如实说读不了") {
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let out = brainDir.appendingPathComponent("history.out")
            let deadline = Date().addingTimeInterval(10)
            while Date() < deadline {
                if let text = try? String(contentsOf: out, encoding: .utf8), !text.isEmpty {
                    guard text.contains("还没有给出会话文件") || text.contains("只支持带手机控制") || text.contains("没有打开的卡片") else {
                        throw E2EError.product("history 的输出不是预期的“读不了”说明：\(text)")
                    }
                    return text
                }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            throw E2EError.product("大脑没有执行 history 命令（history.out 为空）")
        }

        try await session.step("Esc 关闭后，⌘⇧B 快捷键同样能打开面板，再 Esc 关闭") {
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(absent: ["保留最近"], timeout: 6))
            var shown = false
            for _ in 0..<3 where !shown {
                try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
                shown = (try? await session.wait(Expectation(allOf: ["保留最近"], timeout: 5))) != nil
            }
            guard shown else {
                throw E2EError.product("项目窗口里 ⌘⇧B 三次都没有打开面板：\(try await session.capture().summary)")
            }
            try session.save(try await session.capture(), "06-shortcut-open.png")
            return "快捷键可用"
        }

        try await session.step("Esc 关闭幕僚活动面板") {
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(absent: ["保留最近"], timeout: 6))
            return "面板已关闭"
        }
    }
}
