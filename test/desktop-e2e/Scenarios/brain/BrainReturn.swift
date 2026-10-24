import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 回来摘要：用户离开一阵，期间有 Agent 完成，回到窗口时大脑收到一条“回来摘要”提示。
/// 离开阈值缩短到 5 秒（`NORMA_BRAIN_AWAY_SECS`），窗口失焦用控制通道的 deactivate 模拟。
@MainActor
public enum BrainReturn {
    public nonisolated static let name = "brain-return"

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
        let workspace = "ReturnCheck"

        try await session.step("建立工作空间与项目，脚本化大脑，离开阈值 5 秒") {
            fakeShell = try BrainFixture.installFakeAgents(finalMessage: "登录页已经做完，12 个测试全部通过。")
            brainDir = try BrainFixture.installScriptedBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_BRAIN_AWAY_SECS", "5", 1)
            setenv("NORMA_BRAIN_QUIET_SECS", "600", 1)
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

        try await session.step("进入项目，创建一个智能体后立刻“离开”") {
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
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame)
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            // 此后不再有任何输入：输入会重新激活窗口。Alan 约 8 秒后完成，全部发生在“离开”期间。
            try await session.deactivate()
            return "窗口已标记为离开"
        }

        try await session.step("离开约 14 秒后回来，大脑收到含 Alan 结果的“回来摘要”") {
            try await Task.sleep(nanoseconds: 14_000_000_000)
            try await session.activate()
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let log = brainDir.appendingPathComponent("prompt.log")
            let deadline = Date().addingTimeInterval(20)
            var text = ""
            while Date() < deadline {
                text = (try? String(contentsOf: log, encoding: .utf8)) ?? ""
                if text.contains("【回来摘要】") { break }
                try await Task.sleep(nanoseconds: 400_000_000)
            }
            guard text.contains("【回来摘要】") else {
                throw E2EError.product("回来后没有收到回来摘要：\(text.prefix(800))")
            }
            guard text.contains("登录页已经做完") else {
                throw E2EError.product("回来摘要里没有离开期间 Alan 的结果：\(text.prefix(800))")
            }
            return "收到回来摘要，含离开期间的结果"
        }
    }
}
