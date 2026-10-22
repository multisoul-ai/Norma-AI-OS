import E2EKit
import Fixtures
import Foundation

/// 智能体设置页：卡片网格 + 右侧编辑抽屉，核对收起/展开两种布局。
@MainActor
public enum SettingsAgentsLibrary {
    public nonisolated static let name = "settings-agents-library"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "AgentsCheck"

        try await session.step("建立含项目区的隔离工作空间并创建两个智能体") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace", "Create your first"], timeout: 25))
            try await session.act("打开创建对话框", attempts: 4, expect: Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3.5)) {
                let launch = try await session.capture()
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) / 2, y: 515), on: launch)
            }
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
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let entered = try await session.capture()
            if entered.contains("Feature Guide") || entered.contains("Skip Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Feature Guide", "Skip Guide"], timeout: 6))
            }
            for command in ["/new claude", "/new codex"] {
                let frame = try await session.capture()
                try await session.click(
                    local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                    on: frame
                )
                try await session.selectAll()
                try await session.type(command)
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 2_500_000_000)
            }
            return try await session.capture().summary
        }

        try await session.step("打开设置的智能体页：抽屉默认收起，网格铺满") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["交流风格", "Communication Style"], timeout: 10))
            try await session.scroll(xFraction: 0.25, yFraction: 0.5, deltaY: -600)
            let sidebar = try await session.capture()
            try session.save(sidebar, "00-sidebar.png")
            let entries = sidebar.texts.filter {
                ($0.value.hasSuffix("Agents") || $0.value.hasSuffix("智能体"))
                    && !$0.value.contains("Check") && $0.center.x < sidebar.windowFrame.width * 0.4
                    && $0.center.y > sidebar.windowFrame.height * 0.4
            }
            guard let target = entries.first else {
                throw E2EError.product("侧栏找不到智能体：\(sidebar.summary)")
            }
            try await session.click(local: target.center, on: sidebar)
            let frame = try await session.wait(Expectation(anyOf: ["New Agent", "新建智能体", "No project agents"], timeout: 8))
            try session.save(frame, "01-agents-grid.png")
            return "卡片网格显示，抽屉收起"
        }

        try await session.step("点击卡片：模态抽屉从右侧覆盖网格") {
            let frame = try await session.capture()
            guard let card = frame.pick(["Claude"], .right) else {
                throw E2EError.product("找不到智能体卡片：\(frame.summary)")
            }
            try await session.click(local: card.center, on: frame)
            let opened = try await session.wait(Expectation(anyOf: ["Save Changes", "Runtime Profile", "运行时"], timeout: 6))
            try session.save(opened, "02-agents-drawer.png")
            return "抽屉已覆盖展开"
        }
    }
}
