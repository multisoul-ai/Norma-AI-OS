import E2EKit
import Fixtures
import Foundation

/// 在真实项目画布上检查抽屉占位与左侧输入归属。
@MainActor
public enum CanvasProjectDrawers {
    public nonisolated static let name = "canvas-project-drawers"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
        }
        let workspace = "ProjectDrawerCheck"

        try await session.step("建立含项目区的隔离工作空间") {
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
            guard let dataRoot = session.dataRoot else {
                throw E2EError.env("隔离数据目录丢失")
            }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入真实项目画布") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
            }
            // 全新数据目录的左抽屉默认收起；进入项目不得改写这一状态。
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["当前对话"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await Task.sleep(nanoseconds: 120_000_000)
            let entering = try await session.capture()
            guard !entering.contains("当前对话") else {
                throw E2EError.product("项目进入过渡帧凭空绘制了左侧 Norma 对话栏：\(entering.summary)")
            }
            try session.save(entering, "00-project-transition.png")
            // 小字保存状态易被 Vision 误识别；项目面包屑与后续真实卡片是验收证据。
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            // 预览卡片与真实画布有缩放交接；只保存镜头落稳后的最终布局。
            try await Task.sleep(nanoseconds: 700_000_000)
            let project = try await session.capture()
            guard !project.contains("当前对话") else {
                throw E2EError.product("进入项目后左抽屉状态被改写（进入前为收起）：\(project.summary)")
            }
            try session.save(project, "01-project-left-closed.png")
            return project.summary
        }

        try await session.step("在项目画布创建真实终端卡片") {
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/terminal")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 700_000_000)
            let card = try await session.capture()
            guard (card.exact("终端") != nil || card.exact("Terminal") != nil),
                  !card.contains("请先打开项目"),
                  !card.contains("项目目录不可用") else {
                throw E2EError.product("执行 /terminal 后画布没有终端卡片：\(card.summary)")
            }
            try session.save(card, "02-project-terminal.png")
            return "项目画布出现真实 Terminal 卡片"
        }

        try await session.step("在项目画布创建真实 Agent 卡片") {
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new codex")
            try session.save(try await session.capture(), "03-agent-command.png")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 500_000_000)
            try session.save(try await session.capture(), "03-agent-after-enter.png")
            let card = try await waitForRealAgent(session)
            try session.save(card, "03-project-agent.png")
            return "项目画布同时出现 Agent 与 Terminal 卡片"
        }

        try await session.step("左右抽屉同时展开时项目仍可见") {
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: 98, y: 26), on: frame)
            _ = try await session.wait(Expectation(allOf: ["当前对话", "E2E Project"], timeout: 8))
            let leftOpened = try await session.capture()
            try await session.click(local: CGPoint(x: Double(leftOpened.windowFrame.width) - 28, y: 26), on: leftOpened)
            _ = try await session.wait(Expectation(allOf: ["E2E Project", "在这里浏览内容", "当前对话"], timeout: 8))
            try await Task.sleep(nanoseconds: 700_000_000)
            let opened = try await session.capture()
            try session.save(opened, "04-project-both-open.png")
            guard let agent = canvasText(opened, matching: ["Alan"], left: 397, right: 807),
                  let terminal = canvasText(opened, matching: ["终端", "Terminal"], left: 397, right: 807),
                  agent.center.y > terminal.center.y + 200 else {
                throw E2EError.product("双侧抽屉展开后中央未完整呈现纵排 Agent／Terminal：\(opened.summary)")
            }
            return "项目画布保留在双侧抽屉之间"
        }

        try await session.step("窄画布可滚动查看下方卡片") {
            let before = try await session.capture()
            guard let agentBefore = canvasText(before, matching: ["Alan"], left: 397, right: 807) else {
                throw E2EError.product("滚动前找不到下方 Agent 卡片：\(before.summary)")
            }
            try await session.scroll(local: CGPoint(x: 430, y: 360), pixels: 320, on: before)
            try await Task.sleep(nanoseconds: 350_000_000)
            let after = try await session.capture()
            try session.save(after, "04b-project-scrolled.png")
            guard let agentAfter = canvasText(after, matching: ["Alan"], left: 397, right: 807),
                  agentAfter.center.y < agentBefore.center.y - 80 else {
                throw E2EError.product("滚轮未将下方 Agent 卡片移入可操作区域：\(after.summary)")
            }
            // The OCR hit is the Agent title text inside its title bar. Scrolling over
            // the selected Agent body belongs to terminal scrollback instead.
            try await session.scroll(
                local: CGPoint(x: 430, y: agentAfter.center.y),
                pixels: -320,
                on: after
            )
            try await Task.sleep(nanoseconds: 350_000_000)
            let restored = try await session.capture()
            try session.save(restored, "04c-project-scroll-restored.png")
            guard let agentRestored = canvasText(restored, matching: ["Alan"], left: 397, right: 807),
                  abs(agentRestored.center.y - agentBefore.center.y) < 60 else {
                throw E2EError.product("返回滚轮未恢复卡片原位置：\(restored.summary)")
            }
            return "滚轮能到达下方卡片，并可返回原位置"
        }

        try await session.step("左侧输入作用于当前项目") {
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: 120, y: Double(frame.windowFrame.height) - 38), on: frame)
            try await session.type("项目对话输入验收")
            let typed = try await session.wait(Expectation(allOf: ["项目对话输入验收", "E2E Project"], timeout: 5))
            try session.save(typed, "05-project-chat-input.png")
            return "左侧对话输入可在当前项目里接收键盘文字"
        }

        try await session.step("总览与项目往返时左抽屉保持展开") {
            let inProject = try await session.capture()
            guard let crumb = inProject.texts.first(where: { text in
                text.center.y < 52 && text.center.x > 397 && text.value.contains(workspace)
            }) else {
                throw E2EError.product("项目顶栏找不到工作空间面包屑：\(inProject.summary)")
            }
            try await session.click(local: crumb.center, on: inProject)
            try await Task.sleep(nanoseconds: 900_000_000)
            let overview = try await session.capture()
            try session.save(overview, "05b-back-to-overview-left-open.png")
            guard overview.contains("当前对话"), overview.contains("E2E Project") else {
                throw E2EError.product("返回总览后左抽屉被收起或总览未出现：\(overview.summary)")
            }
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let reentered = try await session.capture()
            try session.save(reentered, "05c-reenter-project-left-open.png")
            guard reentered.contains("当前对话"), reentered.contains("E2E Project") else {
                throw E2EError.product("再次进入项目后左抽屉被强制收起：\(reentered.summary)")
            }
            return "总览→项目→总览→项目，左抽屉始终展开"
        }

        try await session.step("收起右抽屉后恢复宽画布卡片布局") {
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: 834, y: 26), on: frame)
            _ = try await session.wait(Expectation(allOf: ["E2E Project", "Alan"], absent: ["在这里浏览内容"], timeout: 8))
            try await Task.sleep(nanoseconds: 700_000_000)
            let closed = try await session.capture()
            try session.save(closed, "06-project-right-closed.png")
            guard let agent = canvasText(closed, matching: ["Alan"], left: 397, right: 1260),
                  let terminal = canvasText(closed, matching: ["终端", "Terminal"], left: 397, right: 1260),
                  agent.center.x > terminal.center.x + 120,
                  abs(agent.center.y - terminal.center.y) < 100 else {
                throw E2EError.product("右抽屉收起后中央卡片未恢复并排：\(closed.summary)")
            }
            return "右抽屉收起后项目卡片恢复宽画布布局"
        }
    }
}

/// 只接受落在中央画布范围内的卡片文字，避免左侧概览和右侧内容造成 OCR 假阳性。
private func canvasText(_ frame: Frame, matching names: [String], left: CGFloat, right: CGFloat) -> VisibleText? {
    frame.texts.first { text in
        text.center.x > left && text.center.x < right
            && text.center.y > 55 && text.center.y < frame.windowFrame.height - 70
            && names.contains { text.value.localizedCaseInsensitiveContains($0) }
    }
}

@MainActor
private func waitForRealAgent(_ session: Session) async throws -> Frame {
    let deadline = Date().addingTimeInterval(12)
    var last = ""
    while Date() < deadline {
        let frame = try await session.capture()
        last = frame.summary
        if (frame.contains("Agent Alan") || frame.exact("Agent") != nil || frame.exact("Codex") != nil)
            && !frame.contains("新建 Agent")
            && !frame.contains("操作失败") {
            return frame
        }
        try await Task.sleep(nanoseconds: 200_000_000)
    }
    throw E2EError.product("执行 /new codex 后画布没有 Agent 卡片：\(last)")
}
