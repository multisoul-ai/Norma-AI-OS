import E2EKit
import Fixtures
import Foundation

/// 从项目卡片焦点触发想法快捷键，覆盖空待办、卡片缺失与成功派发。
/// NORMA_E2E_INPUT=ns 时经过真实 AppKit 按键入口；不需要真实模型。
@MainActor
public enum WorkspaceIdeasProjectFocus {
    public nonisolated static let name = "workspace-ideas-project-focus"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "ProjectFocus"
        let body = "Focus shortcut task"
        let digits: [(UInt16, Int)] = [(18, 1), (19, 2), (20, 3), (21, 4), (23, 5)]

        try await session.step("建立隔离项目与一条待办，关闭功能引导") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch(language: "en")
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 25))
            session.stop()
            let database = try session.databaseURL()
            try Catalog.suppressFeatureGuide(database: database)
            try Catalog.seedWorkspace(database: database, name: workspace)
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try Catalog.seedIdea(database: database, workspaceName: workspace, body: body, position: 1024)
            try session.launch(language: "en")
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await session.click(matching: ["Open", "打开"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["All projects"], timeout: 12))
            try await Task.sleep(nanoseconds: 1_000_000_000)
            try await session.shortcut(key: 19, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Claude", "claude"], timeout: 12))
            return "已创建项目卡片与一条待办"
        }

        var savedFocusProof = false
        func focusProject() async throws {
            let frame = try await session.capture()
            // 唯一卡片占画布主体；避开左侧抽屉、标题与底部输入栏，点击终端正文。
            try await session.click(local: CGPoint(x: frame.windowFrame.width * 0.75,
                                                  y: frame.windowFrame.height * 0.5), on: frame)
            // 终端正文收到标记才算聚焦成功，避免宿主接收快捷键时也能假通过。
            let marker = "FocusProbe"
            try await session.type(marker)
            let focused = try await session.wait(Expectation(allOf: [marker], timeout: 5))
            guard focused.texts.contains(where: {
                $0.value.contains(marker) && $0.center.x > focused.windowFrame.width * 0.5
                    && $0.center.y < focused.windowFrame.height - 90
            }) else {
                throw E2EError.product("焦点标记未出现在项目终端正文：\(focused.summary)")
            }
            if !savedFocusProof {
                try session.save(focused, "00-project-focus.png")
                savedFocusProof = true
            }
            for _ in marker { try await session.shortcut(key: 51, modifiers: []) }
            _ = try await session.wait(Expectation(absent: [marker], timeout: 5))
        }

        try await session.step("项目正文有焦点时 Ctrl+2～5 显示缺失卡片提示，不误派发") {
            for (key, digit) in digits.dropFirst() {
                try await focusProject()
                try await session.shortcut(key: key, modifiers: [0x3B])
                let frame = try await session.wait(Expectation(anyOf: ["This project has no agent card \(digit)", "当前项目没有第 \(digit) 张智能体卡片"], timeout: 5))
                let rows = try Catalog.ideas(database: session.databaseURL())
                guard rows.count == 1, rows[0].state == "todo", rows[0].taskRef.isEmpty else {
                    throw E2EError.product("Ctrl+\(digit) 不应派发：\(rows)")
                }
                if digit == 5 { try session.save(frame, "01-missing-card.png") }
            }
            return "缺失卡片提示正确，原待办未变"
        }

        try await session.step("项目正文有焦点时 Ctrl+1 成功派发一次") {
            try await focusProject()
            try await session.shortcut(key: 18, modifiers: [0x3B])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            let rows = try Catalog.ideas(database: session.databaseURL())
            guard rows.count == 1, rows[0].body == body, rows[0].state == "assigned",
                  !rows[0].taskRef.isEmpty, !rows[0].taskRef.hasPrefix("idea:") else {
                throw E2EError.product("应派发唯一想法并保存真实任务引用：\(rows)")
            }
            try session.save(try await session.capture(), "02-assigned.png")
            return rows[0].taskRef
        }

        try await session.step("待办耗尽后项目正文 Ctrl+1～5 均显示空待办提示，任务引用不变") {
            let before = try Catalog.ideas(database: session.databaseURL()).map(\.taskRef)
            for (key, _) in digits {
                try await focusProject()
                try await session.shortcut(key: key, modifiers: [0x3B])
                _ = try await session.wait(Expectation(anyOf: ["No ideas to assign", "没有待办想法可派发"], timeout: 5))
            }
            guard try Catalog.ideas(database: session.databaseURL()).map(\.taskRef) == before else {
                throw E2EError.product("空待办快捷键不应重复派发")
            }
            try session.save(try await session.capture(), "03-empty-notice.png")
            return "连续快捷键未崩溃、未重复派发"
        }
    }
}
