import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 汇报降噪：普通成功先进静默池，攒满窗口后以“静默记录”交给大脑，而不是“必须汇报”的事件。
/// 窗口缩短到 3 秒（`NORMA_BRAIN_QUIET_SECS`），脚本化的大脑把收到的提示写进 `prompt.log`。
@MainActor
public enum BrainQuiet {
    public nonisolated static let name = "brain-quiet"

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
        let workspace = "QuietCheck"

        try await session.step("建立工作空间与项目，脚本化大脑，静默窗口 3 秒") {
            fakeShell = try BrainFixture.installFakeAgents(finalMessage: "登录页已经做完，12 个测试全部通过。")
            brainDir = try BrainFixture.installScriptedBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_BRAIN_QUIET_SECS", "3", 1)
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

        try await session.step("Alan 平平地完成后，大脑收到的是“静默记录”，不是要求汇报的事件") {
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let log = brainDir.appendingPathComponent("prompt.log")
            let deadline = Date().addingTimeInterval(90)
            var text = ""
            while Date() < deadline {
                text = (try? String(contentsOf: log, encoding: .utf8)) ?? ""
                if text.contains("【静默记录】") { break }
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            guard text.contains("【静默记录】") else {
                throw E2EError.product("没有收到静默记录提示：\(text.prefix(800))")
            }
            guard text.contains("登录页已经做完") else {
                throw E2EError.product("静默记录里没有 Alan 的结果：\(text.prefix(800))")
            }
            guard !text.contains("【事件】") else {
                throw E2EError.product("普通成功不应走立即汇报的事件提示：\(text.prefix(800))")
            }
            guard text.contains("默认不需要告诉用户") else {
                throw E2EError.product("静默记录没有写明默认不汇报：\(text.prefix(800))")
            }
            return "收到静默记录，且没有立即汇报的事件"
        }
    }
}
