import E2EKit
import Fixtures
import Foundation

/// 后台大脑：Agent 完成后，真实的 ACP 适配器（Claude Code）收到事件，
/// 通过 `norma brain report` 把汇报文字交回 App 的播报队列。
@MainActor
public enum BrainReport {
    public nonisolated static let name = "brain-report"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            // 失败时也留下大脑的记录和适配器日志，方便排查。
            if let brainDir, let output = session.output {
                for name in ["decisions.jsonl", "approvals.jsonl", "adapter.log"] {
                    try? FileManager.default.copyItem(
                        at: brainDir.appendingPathComponent(name),
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
            fakeShell = try BrainFixture.installFakeAgents()
            brainDir = try BrainFixture.installBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_BRAIN_ADAPTER_LOG", brainDir!.appendingPathComponent("adapter.log").path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 4))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
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
            try await dismissGuide(session)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            try await dismissGuide(session)
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 1_500_000_000)
            return try await session.capture().summary
        }

        try await session.step("大脑启动，并在 Agent 完成后通过 norma brain report 交回播报") {
            let log = try appLog(session)
            try await waitForLog(log, containing: "Norma 大脑已启动", timeout: 20)
            // Agent 约 4 秒后 Stop；大脑用真实 Claude Code 处理，首次约 10 到 40 秒。
            try await waitForLog(log, containing: "大脑播报", timeout: 150)
            guard let brainDir, let output = session.output else { throw E2EError.env("大脑目录丢失") }
            // 日志里出现汇报时，这一轮可能还没结束；等主会话把回复写进决策记录。
            let decisions = brainDir.appendingPathComponent("decisions.jsonl")
            var text = ""
            let deadline = Date().addingTimeInterval(90)
            while Date() < deadline {
                text = (try? String(contentsOf: decisions, encoding: .utf8)) ?? ""
                if text.contains("Main") { break }
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            guard text.contains("Main") else {
                throw E2EError.product("决策日志里没有主会话的回复：\(decisions.path)")
            }
            return "日志与决策记录都出现了大脑的汇报"
        }
    }

    private static func dismissGuide(_ session: Session) async throws {
        for _ in 0..<3 {
            try await Task.sleep(nanoseconds: 1_500_000_000)
            let frame = try await session.capture()
            let showing = ["Feature Guide", "功能引导", "功能引号", "Skip Guide", "跳过引导"].contains { frame.contains($0) }
            guard showing else { return }
            try await session.shortcut(key: 53, modifiers: [])
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
