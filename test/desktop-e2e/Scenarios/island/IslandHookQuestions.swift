import E2EKit
import Fixtures
import Foundation

/// Grok 脚本 Agent：岛内选项、自由回复、终端作答收卡、英文 Needs you、native_first 不拦截。
@MainActor
public enum IslandHookQuestions {
    public nonisolated static let name = "island-hook-questions"

    public static func run(_ session: Session) async throws {
        let hook = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("multisoul-agent-hook").path
        let fixture = try HookQuestionFixture.install(hookBinary: hook)
        var project: URL?
        let previous = ProcessInfo.processInfo.environment["ZDOTDIR"]
        defer {
            session.stop()
            if let output = session.output {
                if let data = try? Data(contentsOf: fixture.root.appendingPathComponent("hook.log")) {
                    try? data.write(to: output.appendingPathComponent("hook.log"))
                }
            }
            if let previous { setenv("ZDOTDIR", previous, 1) } else { unsetenv("ZDOTDIR") }
            unsetenv("NORMA_E2E_NO_ACTIVATE")
            for path in [fixture.root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", fixture.shell.path, 1)
        try await session.step("启动隔离 Grok 脚本卡片") {
            try await IslandHookSupport.boot(session, language: "zh-Hans", shell: fixture.shell, workspace: "Hook questions", project: &project)
            try await IslandHookSupport.spawn(session, command: "/new grok", provider: "grok", root: fixture.root)
            return "生产 hook 二进制由脚本 Grok 调用，不使用真实模型"
        }
        let island = try await IslandHookSupport.island(session)
        try await session.step("中文岛出现 Grok 题，选项 Blue 以 deny 回给 hook") {
            try IslandHookSupport.trigger(fixture.root, provider: "grok", cycle: 1, mode: "choice")
            try await IslandHookSupport.waitForAlert(session, number: island)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "appeared", text: "岛展开并显示需要你或需要你处理，待处理计数为 1；题目是 Spike color 1?，有 Blue 和 Green 两个选项以及自由回复")
            ]), "00-grok-appeared.png")
            try await session.visualClick("在 Spike color 1? 下选择 Blue 单选项")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "choice", text: "Blue 为选中的单选项，Green 未选中，发送回复按钮可用，题目 Spike color 1? 仍在")
            ]), "01-grok-choice.png")
            try await session.visualClick("点击发送回复按钮")
            let meta = try await IslandHookSupport.waitMeta(fixture.root, provider: "grok", cycle: 1)
            try IslandHookSupport.assertGrokDeny(meta, reason: "The user answered Spike color 1? with Blue.")
            try await IslandHookSupport.waitForCompact(session, number: island)
            return "Grok stdout 只有 decision=deny 与 Blue reason"
        }
        try await session.step("自由回复 Cobalt blue 进入 Grok reason") {
            try IslandHookSupport.trigger(fixture.root, provider: "grok", cycle: 2, mode: "custom")
            try await IslandHookSupport.waitForAlert(session, number: island)
            try await session.visualClick("点击自由回复输入框")
            try await session.type("Cobalt blue")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "custom", text: "Spike color 2? 的自由回复输入框完整显示 Cobalt blue，包含空格，发送回复按钮可用")
            ]), "02-grok-custom.png")
            try await session.visualClick("点击发送回复按钮")
            let meta = try await IslandHookSupport.waitMeta(fixture.root, provider: "grok", cycle: 2)
            try IslandHookSupport.assertGrokDeny(meta, reason: "The user answered Spike color 2? with Cobalt blue.")
            try await IslandHookSupport.waitForCompact(session, number: island)
            return "自定义答案经控制通道键入，reason 含 Cobalt blue"
        }
        try await session.step("终端作答后岛卡消失且 hook 不再 deny") {
            try IslandHookSupport.trigger(fixture.root, provider: "grok", cycle: 3, mode: "terminal")
            try await IslandHookSupport.waitForAlert(session, number: island)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "terminal-open", text: "岛再次展开，题目是 Spike color 3?，Blue 和 Green 仍可选择")
            ]), "03-grok-terminal-open.png")
            try IslandHookSupport.close(fixture.root, provider: "grok", cycle: 3)
            let meta = try await IslandHookSupport.waitMeta(fixture.root, provider: "grok", cycle: 3)
            try IslandHookSupport.assertEmptyStdout(meta, what: "终端作答")
            try await IslandHookSupport.waitForCompact(session, number: island)
            return "PostToolUse UserAnswered 收卡，等待中的 hook stdout 为空"
        }
        try await session.step("英文岛显示 Needs you，Blue 仍走 deny") {
            try await IslandHookSupport.relaunch(session, language: "en", workspace: "Hook questions")
            try await IslandHookSupport.spawn(session, command: "/new grok", provider: "grok", root: fixture.root)
            let english = try await IslandHookSupport.island(session)
            try IslandHookSupport.trigger(fixture.root, provider: "grok", cycle: 1, mode: "choice")
            try await IslandHookSupport.waitForAlert(session, number: english)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "english", text: "岛显示 Needs you 和 1 pending；题目 Spike color 1?，选项 Blue 与 Green 可见，没有中文需要你处理残留")
            ]), "04-grok-english.png")
            try await session.visualClick("选择 Spike color 1? 下的 Blue 单选项")
            try await session.visualClick("点击 Send reply 按钮")
            let meta = try await IslandHookSupport.waitMeta(fixture.root, provider: "grok", cycle: 1)
            try IslandHookSupport.assertGrokDeny(meta, reason: "The user answered Spike color 1? with Blue.")
            try await IslandHookSupport.waitForCompact(session, number: english)
            return "英文壳层与中文同一条 deny 回执"
        }
        try await session.step("交给卡片回答时 hook 立即空返回") {
            guard let main = try await session.nativeWindows().first(where: { $0.width > 900 }) else {
                throw E2EError.product("缺少主窗口")
            }
            try await session.selectNativeWindow(main.number)
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("点击设置左侧 Norma 分组里的 Island 菜单项")
            try await session.visualClick("点击 Answer questions on the agent card 这一行右侧的开关，把它打开；不要改其他开关")
            try await IslandHookSupport.nativeQuestions(session, expected: true)
            let english = try await IslandHookSupport.island(session)
            try IslandHookSupport.trigger(fixture.root, provider: "grok", cycle: 2, mode: "native")
            let meta = try await IslandHookSupport.waitMeta(fixture.root, provider: "grok", cycle: 2, seconds: 25)
            try IslandHookSupport.assertEmptyStdout(meta, what: "native_first")
            if let elapsed = (meta["elapsed"] as? NSNumber)?.doubleValue, elapsed > 15 {
                throw E2EError.product("native_first 没有马上放开 hook：\(elapsed)")
            }
            try IslandHookSupport.close(fixture.root, provider: "grok", cycle: 2)
            try await IslandHookSupport.waitForCompact(session, number: english)
            return "未在岛内点击，hook stdout 为空；随后终端回执收卡"
        }
    }
}
