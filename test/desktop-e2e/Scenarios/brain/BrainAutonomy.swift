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
        let workspace = "AutonomyCheck"

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
                if frame.contains("补上登录页的测试") { break }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            try session.save(frame, "01-autonomy-row.png")
            for needle in ["Norma 自主处理", "派发", "补上登录页的测试"] {
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
                if try await session.capture().contains("以后这类先问我") { break }
                try await session.scroll(xFraction: 0.2, yFraction: 0.7, deltaY: -240)
            }
            try session.save(try await session.capture(), "03-autonomy-actions.png")
            try await session.click(matching: ["以后这类先问我", "以后这类"], pick: .bottom)
            let deadline = Date().addingTimeInterval(6)
            while Date() < deadline {
                if let text = try? String(contentsOf: config, encoding: .utf8),
                   text.contains("\"assign_followups\": false") {
                    return text
                }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            throw E2EError.product("配置里没有出现 assign_followups=false")
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
