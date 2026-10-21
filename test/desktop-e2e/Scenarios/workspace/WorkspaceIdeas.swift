import E2EKit
import Fixtures
import Foundation

/// 想法清单：⌘⇧I 打开、把想法拖到智能体卡片派发，核对数据与画面。
@MainActor
public enum WorkspaceIdeas {
    public nonisolated static let name = "workspace-ideas"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "IdeasCheck"
        let first = "检查登录方案"
        let second = "整理注册流程"

        try await session.step("建立含项目区与两条想法的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace", "Create your first"], timeout: 25))
            // 启动页的「创建工作空间」按钮位于窗口水平居中、约 515px 处；OCR 偶尔识别不出，这里按位置点击。
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
            let database = dataRoot.appendingPathComponent("app.sqlite")
            try Catalog.seedIdea(database: database, workspaceName: workspace, body: first, position: 1024)
            try Catalog.seedIdea(database: database, workspaceName: workspace, body: second, position: 2048)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "已写入 2 条想法"
        }

        try await session.step("进入项目并创建一个智能体") {
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
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            return try await session.capture().summary
        }

        try await session.step("⌘⇧I 打开想法清单，看到两条待办") {
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            let ideas = try await session.wait(
                Expectation(allOf: ["想法清单", first, second], timeout: 8)
            )
            try session.save(ideas, "01-ideas.png")
            return ideas.summary
        }

        try await session.step("把第一条想法拖到智能体卡片") {
            let frame = try await session.capture()
            guard let row = frame.pick([first], .top) else {
                throw E2EError.product("找不到想法行「\(first)」：\(frame.summary)")
            }
            // 假智能体卡片标题行含临时项目目录名；落点取标题行正下方的卡片正文区。
            guard let title = frame.pick(["norma-canvas-e2e"], .bottom) else {
                throw E2EError.product("找不到智能体卡片标题：\(frame.summary)")
            }
            let target = CGPoint(x: title.center.x, y: title.center.y + 150)
            try await session.drag(local: row.center, to: target, on: frame)
            try await Task.sleep(nanoseconds: 350_000_000)
            try session.save(try await session.capture(), "02-meteor.png")
            try await Task.sleep(nanoseconds: 1_800_000_000)
            let done = try await session.capture()
            try session.save(done, "03-assigned.png")
            return done.summary
        }

        try await session.step("数据与画面：想法变为已指派并带任务引用") {
            let database = try session.databaseURL()
            let ideas = try Catalog.ideas(database: database)
            guard let assigned = ideas.first(where: { $0.body == first }) else {
                throw E2EError.product("库里找不到想法「\(first)」：\(ideas)")
            }
            guard assigned.state == "assigned", !assigned.taskRef.isEmpty else {
                throw E2EError.product("想法没有变成已指派或缺少任务引用：\(assigned)")
            }
            guard let untouched = ideas.first(where: { $0.body == second }), untouched.state == "todo" else {
                throw E2EError.product("第二条想法不应被改动：\(ideas)")
            }
            let screen = try await session.wait(Expectation(allOf: ["已指派"], timeout: 6))
            try session.save(screen, "04-list-after.png")
            return "\(assigned.state) \(assigned.taskRef)"
        }
    }
}
