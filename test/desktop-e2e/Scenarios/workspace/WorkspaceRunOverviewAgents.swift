import E2EKit
import Fixtures
import Foundation

/// 有智能体运行时：右上角状态显示与左栏运行概览对照原型的真实 App 验收。
@MainActor
public enum WorkspaceRunOverviewAgents {
    public nonisolated static let name = "workspace-run-overview-agents"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var otherRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let otherRoot { try? FileManager.default.removeItem(at: otherRoot) }
        }
        let workspace = "AgentOverviewCheck"

        try await session.step("建立含项目区的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            setenv("NORMA_FAKE_APPROVAL", "1", 1)
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
            let other = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
                .appendingPathComponent("norma-canvas-e2e-other-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: other, withIntermediateDirectories: true)
            otherRoot = other
            try Catalog.seedArea(
                database: dataRoot.appendingPathComponent("app.sqlite"),
                workspaceName: workspace,
                root: other,
                displayName: "设计草稿"
            )
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 2, linkedTo: workspace).summary
        }

        try await session.step("进入项目并创建六个智能体") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                // 工作空间卡片右下角的「打开」按钮：文字识别会与项目占位文案混淆，改用位置点击。
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
            // 功能引导会盖住指令栏：先关掉，否则命令发不进去。
            let guide = try await session.capture()
            if guide.contains("Feature Guide") || guide.contains("功能引导") || guide.contains("Skip Guide") || guide.contains("跳过引导") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            for command in ["/new claude", "/new codex", "/new cursor", "/new opencode", "/new grok", "/new kimi"] {
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
            return try await session.capture().summary
        }

        try await session.step("右上角状态胶囊显示跨项目汇总（运行中 / 待开始 / 待处理）") {
            try await Task.sleep(nanoseconds: 9_000_000_000)
            let capsule = try await session.capture()
            try session.save(capsule, "01-capsule.png")
            return capsule.summary
        }

        try await session.step("⌘⇧E 看运行概览里的进度行") {
            // 首次进入工作空间会自动弹出功能引导，盖住界面：先关掉它。
            let current = try await session.capture()
            if current.contains("Feature Guide") || current.contains("功能引导") || current.contains("Skip Guide") || current.contains("跳过引导") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            try await Task.sleep(nanoseconds: 900_000_000)
            let overview = try await session.capture()
            try session.save(overview, "02-overview.png")
            guard overview.contains("要你处理") || overview.contains("需要你"), overview.contains("更新") else {
                throw E2EError.product("⌘⇧E 没有打开运行概览或副标题缺少更新时间：\(overview.summary)")
            }
            guard !overview.contains("待审核，待审核"), !overview.contains("待审核 · 待审核") else {
                throw E2EError.product("待审核行标题与芯片重复：\(overview.summary)")
            }
            return overview.summary
        }

        try await session.step("想法与运行中同时展开时，智能体行文字不重叠") {
            let overview = try await session.capture()
            try session.save(overview, "03-both-expanded.png")
            guard overview.contains("Bella") else {
                throw E2EError.product("运行中没有智能体，无法验证重叠：\(overview.summary)")
            }
            return overview.summary
        }
    }
}
