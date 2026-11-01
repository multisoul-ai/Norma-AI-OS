import AppKit
import E2EKit
import Fixtures
import Foundation

/// 想法行的删除按钮与撤销提示条：悬停出现删除图标，删除后出现 Snackbar，点撤销恢复。
@MainActor
public enum WorkspaceIdeasDeleteUndo {
    public nonisolated static let name = "workspace-ideas-delete-undo"

    public static func run(_ session: Session) async throws {
        if session.visualReview { return try await WorkspaceIdeasDeleteUndoVisual.run(session) }
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "IdeasDelUndo"
        let title = "待删除的想法"

        try await session.step("建立含项目区的隔离工作空间") {
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
            return "工作空间已就绪"
        }

        try await session.step("进入项目并打开想法栏，折叠其余三栏") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") || launch.contains("Recently visited") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["Recently visited", "最近访问"], timeout: 12))
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
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            _ = try await session.wait(Expectation(anyOf: ["写下一个想法", "写下一个", "下一个想法"], timeout: 8))
            for title in ["要你处理", "运行中", "待开始"] {
                let frame = try await session.capture()
                guard let heading = frame.texts.filter({ $0.value.contains(title) && $0.center.x < 100 }).min(by: { $0.center.y < $1.center.y }) else {
                    throw E2EError.product("找不到栏标题「\(title)」：\(frame.summary)")
                }
                try await session.click(local: heading.center, on: frame)
                try await Task.sleep(nanoseconds: 400_000_000)
            }
            return try await session.capture().summary
        }

        func focusInput() async throws {
            let frame = try await session.wait(Expectation(anyOf: ["写下一个想法", "写下一个", "下一个想法"], timeout: 8))
            guard let input = frame.pick(["写下一个想法", "写下一个", "下一个想法"], .bottom) else {
                throw E2EError.product("找不到想法输入框：\(frame.summary)")
            }
            try await session.click(local: input.center, on: frame)
        }

        try await session.step("添加一条想法，悬停行后截图删除按钮") {
            try await focusInput()
            try await session.type(title)
            try await session.shortcut(key: 36, modifiers: [])
            let row = try await session.wait(Expectation(anyOf: [title], timeout: 8))
            guard let text = row.pick([title], .top) else { throw E2EError.product("找不到想法行：\(row.summary)") }
            try await session.hover(local: text.center, on: row)
            try session.save(try await session.capture(), "01-row-hover.png")
            return "已悬停"
        }

        try await session.step("点删除图标：库里软删除并出现撤销提示") {
            let frame = try await session.capture()
            guard let pill = frame.pick(["指派", "Assign"], .top) else { throw E2EError.product("悬停后应出现指派按钮：\(frame.summary)") }
            // 删除按钮紧跟在指派胶囊右侧，约 36px 处。
            let target = CGPoint(x: pill.center.x + 42, y: pill.center.y)
            try await session.hover(local: target, on: frame)
            try session.save(try await session.capture(), "02-delete-hover.png")
            try await session.click(local: target, on: frame)
            let after = try await session.wait(Expectation(anyOf: ["撤销", "Undo"], timeout: 6))
            try session.save(after, "03-snackbar.png")
            return after.summary
        }

        try await session.step("点撤销：想法恢复") {
            try await session.click(matching: ["撤销", "Undo"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: [title], absent: ["撤销"], timeout: 8))
            try session.save(try await session.capture(), "04-restored.png")
            return "已恢复"
        }
    }
}
