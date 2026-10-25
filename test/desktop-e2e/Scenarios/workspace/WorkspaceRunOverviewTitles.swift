import E2EKit
import Fixtures
import Foundation

/// 左栏运行概览的卡片标题：优先取 CLI 自己的会话名（Claude ai-title、Codex thread_name），
/// 取不到时回退首条提示词。假 CLI 经真实 Hook 接口上报，见 `FakeTitledAgentShell`。
@MainActor
public enum WorkspaceRunOverviewTitles {
    public nonisolated static let name = "workspace-run-overview-titles"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fake: FakeTitledAgentShell.Install?
        defer {
            session.stop()
            unsetenv("CODEX_HOME")
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        let workspace = "AgentTitleCheck"

        try await session.step("建立隔离工作空间与项目") {
            let install = try FakeTitledAgentShell.install()
            fake = install
            setenv("ZDOTDIR", install.root.path, 1)
            setenv("CODEX_HOME", install.codexHome.path, 1)
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

        try await session.step("进入项目并创建两个 Claude 与一个 Codex 智能体") {
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
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let guide = try await session.capture()
            if guide.contains("Feature Guide") || guide.contains("功能引导") || guide.contains("Skip Guide") || guide.contains("跳过引导") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            for command in ["/new claude", "/new claude", "/new codex"] {
                let frame = try await session.capture()
                try await session.click(
                    local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                    on: frame
                )
                try await session.selectAll()
                try await session.type(command)
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 1_500_000_000)
            }
            // 假 CLI 启动后 2s 提交提示词、再过 1s 落盘会话名并 Stop。
            try await Task.sleep(nanoseconds: 8_000_000_000)
            return try await session.capture().summary
        }

        try await session.step("⌘⇧E 运行概览：有会话名的卡片用会话名作主标题，没有的回退首条提示词") {
            let current = try await session.capture()
            if current.contains("Feature Guide") || current.contains("功能引导") || current.contains("Skip Guide") || current.contains("跳过引导") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            _ = try await session.wait(Expectation(allOf: [FakeTitledAgentShell.claudePrompt], timeout: 20))
            // 待开始分组高度有限，三行放不下：先折叠其余分组，让三行都可见。
            for (header, pick) in [("需要你处理", TextPick.top), ("想法", .top)] {
                let frame = try await session.capture()
                if let item = frame.pick([header], pick) {
                    try await session.click(local: item.center, on: frame)
                    try await Task.sleep(nanoseconds: 400_000_000)
                }
            }
            _ = try await session.wait(Expectation(
                allOf: [FakeTitledAgentShell.claudeTitle, FakeTitledAgentShell.fallbackPrompt],
                timeout: 20
            ))
            let overview = try await session.capture()
            try session.save(overview, "01-overview-titles.png")
            try assertTitle(overview, FakeTitledAgentShell.claudeTitle, above: FakeTitledAgentShell.claudePrompt)
            // 没有会话名的卡片：首条提示词作主标题，且只出现一次（不会再重复成第二行）。
            let fallback = overview.texts.filter { $0.value.localizedCaseInsensitiveContains(FakeTitledAgentShell.fallbackPrompt) }
            guard fallback.count == 1 else {
                throw E2EError.product("回退行应只显示一次首条提示词，实际 \(fallback.count) 次：\(overview.summary)")
            }
            // 待开始分组放不下第三行：在分组内滚动，直到 Codex 行出现。
            var codex = overview
            for _ in 0..<6 where !codex.contains(FakeTitledAgentShell.codexTitle) {
                guard let row = codex.pick([FakeTitledAgentShell.fallbackPrompt], .top) else { break }
                try await session.scroll(local: row.center, pixels: -120, on: codex)
                try await Task.sleep(nanoseconds: 500_000_000)
                codex = try await session.capture()
            }
            try session.save(codex, "02-overview-codex-title.png")
            try assertTitle(codex, FakeTitledAgentShell.codexTitle, above: FakeTitledAgentShell.codexPrompt)
            return codex.summary
        }
    }

    /// 会话名在上、首条提示词退到紧邻的下一行。
    private static func assertTitle(_ frame: Frame, _ title: String, above prompt: String) throws {
        guard let heading = frame.pick([title], .top), let sub = frame.pick([prompt], .top) else {
            throw E2EError.product("缺少会话名「\(title)」或提示词「\(prompt)」：\(frame.summary)")
        }
        guard heading.center.y < sub.center.y, sub.center.y - heading.center.y < 60 else {
            throw E2EError.product("会话名应在首条提示词正上方：\(title)/\(prompt) \(heading.rect) \(sub.rect)")
        }
    }
}
