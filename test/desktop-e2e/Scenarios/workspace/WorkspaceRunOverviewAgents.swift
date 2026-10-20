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
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "AgentOverviewCheck"

        try await session.step("建立含项目区的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
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

        try await session.step("进入项目并创建两个智能体") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            for command in ["/new claude", "/new codex"] {
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

        try await session.step("右上角状态胶囊显示运行中与已完成数") {
            try await Task.sleep(nanoseconds: 9_000_000_000)
            let capsule = try await session.capture()
            try session.save(capsule, "01-capsule.png")
            return capsule.summary
        }

        try await session.step("⌘⇧E 看运行概览里的进度行") {
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            try await Task.sleep(nanoseconds: 900_000_000)
            let overview = try await session.capture()
            try session.save(overview, "02-overview.png")
            guard overview.contains("当前项目") else {
                throw E2EError.product("⌘⇧E 没有打开运行概览：\(overview.summary)")
            }
            return overview.summary
        }
    }
}
