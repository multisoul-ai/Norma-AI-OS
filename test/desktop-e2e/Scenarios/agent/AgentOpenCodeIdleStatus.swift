import E2EKit
import Fixtures
import Foundation

/// Open a real OpenCode card through the workspace and verify its entire Build turn visually.
@MainActor
public enum AgentOpenCodeIdleStatus {
    public nonisolated static let name = "agent-opencode-idle-status"

    public static func run(_ session: Session) async throws {
        guard session.visualReview, let output = session.output else {
            throw E2EError.env("请使用 agent-opencode-idle-status --visual")
        }
        let fixture = try OpenCodeStatusFixture(root: output.appendingPathComponent("opencode-fixture"))
        var project: URL?
        defer {
            session.stop()
            fixture.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
        }

        try await session.step("从工作空间进入真实项目并新建原生 OpenCode 卡片") {
            let workspace = "OpenCode Status Check"
            try await fixture.start(appPath: session.appPath)
            try session.launch(language: "zh-Hans", homeDirectory: fixture.home)
            _ = try await session.capturePixels()
            let database = try session.databaseURL()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            session.stop()
            try Catalog.seedWorkspace(database: database, name: workspace)
            try Catalog.suppressFeatureGuide(database: database)
            project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: workspace)
            try session.launch(language: "zh-Hans", homeDirectory: fixture.home)
            try await session.resize(width: 1280, height: 800)
            try await session.visualClick("点击工作空间列表的 \(workspace) 进入空间。仍在加载时判 wait。")
            try await session.visualClick("点击空间项目列表的 E2E Project 进入项目画布。仍在加载时判 wait。")
            try await session.shortcut(key: 23, modifiers: [0x37]) // Cmd+5: OpenCode.
            return "从工作空间页面打开 E2E Project，使用 Cmd+5 创建 OpenCode 卡"
        }

        try await session.step("未发送任务，原生 Build 页面标题仍是待开始") {
            try await session.visualClick("点击 OpenCode 终端卡内 Build 的空提示词输入区域，使原生终端获得焦点。若原生 TUI 正在启动，判 wait。")
            let frame = try await session.visualCheck([
                VisualCriterion(id: "build-ready", text: "E2E Project 画布上的 OpenCode 原生终端有 Build 和 Question fixture、空输入框，没有任务文本或问答。如果 TUI 正在启动，判 wait。"),
                VisualCriterion(id: "idle-badge", text: "同一张 OpenCode 卡片自身的标题栏明确显示「待开始」，没有「运行中」「等待回答」或 Done。请读卡片标题栏。"),
            ])
            try session.save(frame, "00-idle-build.png")
            return "Build 已可输入，未发送任务"
        }

        try await session.step("发送任务并捕获真实模型的首次生成") {
            try await session.type("NATIVE_FIXTURE_COLOR. Ask one question about the fixture color, then confirm the answer. Do not use other tools.")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await fixture.waitFor("generate")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "first-running", text: "同一卡片自身标题栏显示「运行中」，没有「待开始」「等待回答」或 Done。"),
                VisualCriterion(id: "first-partial", text: "原生终端正在展示首段回复 I will check the fixture color.，还没有选项表单或 NATIVE_FIXTURE_DONE。如果文字尚未绘出，判 wait。"),
            ]), "01-generating.png")
            try fixture.release("generate")
            return "真实模型流已经开始，截图后允许原生 question 工具继续"
        }

        try await session.step("真实 question 等待回答，按 Return 提交第一项 Green") {
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "question-badge", text: "同一卡片自身标题栏显示「等待回答」，没有「运行中」或 Done。若刚开始变更，判 wait。"),
                VisualCriterion(id: "question-prompt", text: "原生终端里的活动问题 Which fixture color should I use? 与选项 Green、Blue 都完整可见，第一项 Green 是键盘当前高亮项。若表单仍在加载，判 wait。"),
            ]), "02-waiting-question.png")
            try await session.shortcut(key: 36, modifiers: [])
            try await fixture.requireGreenAnswer()
            return "原生 TUI 已提交 Green，下一段真实模型请求只收到一个 Green 工具答案"
        }

        try await session.step("回答后同一回合恢复生成") {
            _ = try await fixture.waitFor("resume")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "resume-running", text: "同一卡片自身标题栏恢复「运行中」，没有「等待回答」「待开始」或 Done。"),
                VisualCriterion(id: "resume-partial", text: "活动问题表单已消失，原生终端显示部分回复 Color confirmed.，结尾标记 NATIVE_FIXTURE_DONE 仍未出现。若正文尚未绘出，判 wait。"),
            ]), "03-generating-after-answer.png")
            try fixture.release("resume")
            return "原生问题已被消费，模型流恢复并在截图后正常结束"
        }

        try await session.step("最终回复与 Done 同时出现") {
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "done-badge", text: "同一卡片自身标题栏显示 Done，没有「运行中」「等待回答」或「待开始」。若收尾中判 wait。"),
                VisualCriterion(id: "done-reply", text: "终端可见最终回复 Color confirmed. NATIVE_FIXTURE_DONE，没有活动问答、报错或截断。"),
            ]), "04-completed.png")
            return "同一卡片在原生回合结束后才显示 Done"
        }
    }
}
