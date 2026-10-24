import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// “Norma 自主处理”：脚本化的大脑在第一个智能体完成后给第二个智能体派后续任务，
/// 概览里出现这条记录，并且“为什么”“以后这类先问我”可用。
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

        try await session.step("⌘⇧E 打开概览，等“Norma 自主处理”里出现派发记录") {
            // 快捷键偶尔被刚结束的输入吞掉：没看到“运行概览”就再按一次。
            for _ in 0..<3 {
                try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
                try await Task.sleep(nanoseconds: 1_200_000_000)
                if try await session.capture().contains("运行概览") { break }
            }
            var frame = try await session.capture()
            let deadline = Date().addingTimeInterval(120)
            while Date() < deadline {
                frame = try await session.capture()
                // 小号中文偶尔识别出错字（“登录页”认成“堂家贝”），只认开头的“补上”。
                if frame.contains("派发") && frame.contains("补上") { break }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            try session.save(frame, "01-autonomy-row.png")
            for needle in ["Norma 自主处理", "派发", "补上"] {
                guard frame.contains(needle) else {
                    throw E2EError.product("概览里缺少「\(needle)」：\(frame.summary)")
                }
            }
            return "概览出现自主处理记录"
        }

        try await session.step("点“为什么”，展开大脑给的理由") {
            try await BrainSupport.clickEnd(session, matching: ["为什么"])
            // 栏高有限，展开的内容在栏内下方，向下滚动后才看得到。
            var frame = try await session.capture()
            for _ in 0..<4 where !frame.contains("理由") {
                try await session.scroll(xFraction: 0.2, yFraction: 0.7, deltaY: -240)
                frame = try await session.capture()
            }
            try session.save(frame, "02-autonomy-why.png")
            guard frame.contains("理由") else { throw E2EError.product("展开后没有看到“理由”：\(frame.summary)") }
            return "展开了理由"
        }

        try await session.step("点“以后这类先问我”，配置关闭自行派任务") {
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let config = brainDir.appendingPathComponent("config.json")
            // 展开后的操作按钮在栏内下方，栏高有限时要先在栏内向下滚。
            for _ in 0..<4 {
                // 按钮文字偶尔识别不到，看到紧挨着的“理由”也算到位，避免一直往下滚把它滚出视口。
                let frame = try await session.capture()
                if frame.contains("以后这类先问我") || frame.contains("理由") { break }
                try await session.scroll(xFraction: 0.2, yFraction: 0.7, deltaY: -240)
            }
            try session.save(try await session.capture(), "03-autonomy-actions.png")
            // 同一画面的文字识别时有时无，多取几帧。
            var actions = try await session.capture()
            for _ in 0..<5 where actions.pick(["以后这类先问我", "以后这类", "理由"], .bottom) == nil {
                try await Task.sleep(nanoseconds: 400_000_000)
                actions = try await session.capture()
            }
            if actions.pick(["以后这类先问我", "以后这类"], .bottom) != nil {
                try await session.click(matching: ["以后这类先问我", "以后这类"], pick: .bottom)
            } else if let reason = actions.pick(["理由"], .bottom) {
                // 胶囊里的小号文字偶尔识别不到：按钮行紧贴在“理由”下一行左侧。
                let h = Double(reason.rect.height)
                try await session.click(
                    local: CGPoint(x: Double(reason.rect.minX) + 2 * h, y: Double(reason.rect.maxY) + 1.65 * h),
                    on: actions)
            } else {
                throw E2EError.product("找不到“以后这类先问我”或“理由”：\(actions.summary)")
            }
            try session.save(try await session.capture(), "03b-after-ask.png")
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

        try await session.step("⌘⇧B 弹出大脑活动窗口，展开一轮看到输入与工具，Esc 关闭") {
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            let opened = try await session.wait(Expectation(anyOf: ["大脑活动"], timeout: 8))
            try session.save(opened, "04-brain-trace.png")
            guard opened.contains("主会话") else {
                throw E2EError.product("窗口里没有看到“主会话”卡片：\(opened.summary)")
            }
            try await session.click(matching: ["主会话"], pick: .top)
            try await Task.sleep(nanoseconds: 600_000_000)
            let expanded = try await session.capture()
            try session.save(expanded, "05-brain-trace-expanded.png")
            guard expanded.contains("brain assign") || expanded.contains("补上登录页的测试") else {
                throw E2EError.product("展开后没有看到大脑执行的命令：\(expanded.summary)")
            }
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(absent: ["大脑活动"], timeout: 6))
            return "窗口弹出、展开、关闭都正常"
        }
    }
}
