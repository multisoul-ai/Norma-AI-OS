import E2EKit
import Fixtures
import Foundation

@MainActor
public enum WorkspaceFeatureGuide {
    public nonisolated static let name = "workspace-feature-guide"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
        }
        let name = "GuideCheck"

        try await session.step("建立含项目区的隔离工作空间") {
            try session.launch(language: "en")
            try await openCreateDialog(session, entry: ["创建工作空间", "Create Workspace"])
            try await fill(session, name)
            try await confirmCreate(session, name)
            _ = try await session.wait(Expectation(allOf: [name], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else {
                throw E2EError.env("隔离数据目录丢失")
            }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: name)
            try session.launch(language: "en")
            _ = try await session.wait(Expectation(allOf: [name], timeout: 20))
            return try session.prove(names: [name], areaCount: 1, linkedTo: name).summary
        }

        try await session.step("进入项目画布（首次进入）") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
            }
            let frame = try await session.wait(Expectation(allOf: [name], timeout: 12))
            // 项目区标题小字 OCR 不稳定；区域卡片占满画布，点中心进入。
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) / 2 - 40),
                on: frame
            )
            return "已点击项目区"
        }

        try await session.step("首次进入工作空间自动出现功能引导") {
            let frame = try await session.wait(Expectation(anyOf: ["Feature Guide", "Skip Guide", "Agent Orchestration", "Voice Interaction"], timeout: 20))
            try session.save(frame, "01-auto.png")
            return "已截图"
        }

        try await session.step("下一步切换主题") {
            let before = try await session.capture()
            // 「下一步」固定在弹窗右下角（距窗口右 171、下 54）；OCR 对箭头文字不稳定。
            try await session.click(
                local: CGPoint(x: Double(before.windowFrame.width) - 171, y: Double(before.windowFrame.height) - 54),
                on: before
            )
            let frame = try await session.wait(Expectation(anyOf: ["2 / 11", "2/11"], timeout: 6))
            try session.save(frame, "02-next.png")
            return "已截图"
        }

        try await session.step("方向键翻到最后一页：快捷键速查") {
            for page in 2...10 {
                _ = try await session.wait(Expectation(allOf: ["Playing"], timeout: 10))
                try await Task.sleep(nanoseconds: 900_000_000)
                try session.save(try await session.capture(), "demo-en-\(page).png")
                try await session.shortcut(key: 125, modifiers: [])
                try await Task.sleep(nanoseconds: 350_000_000)
            }
            // 页码小字 OCR 会把 末页页码读错，改用最后一页独有的主按钮与标题。
            _ = try await session.wait(Expectation(allOf: ["Get Started", "Cheat Sheet"], timeout: 6))
            // 页面切换有交叉淡入，等落稳后再截图。
            try await Task.sleep(nanoseconds: 900_000_000)
            let settled = try await session.capture()
            for word in ["Workspaces", "Projects", "Dispatch ideas"] where !settled.contains(word) {
                throw E2EError.product("速查页缺少分组「\(word)」。可见：\(settled.summary)")
            }
            try session.save(settled, "02b-cheat-sheet.png")
            return "已截图"
        }

        try await session.step("Esc 关闭引导") {
            try await session.shortcut(key: 53, modifiers: [])
            try await Task.sleep(nanoseconds: 600_000_000)
            let frame = try await session.capture()
            guard !frame.contains("Skip Guide") else {
                throw E2EError.product("Esc 后功能引导仍可见。可见：\(frame.summary)")
            }
            try session.save(frame, "03-closed.png")
            return "已关闭"
        }

        try await session.step("Dock 占位提示包含 /help") {
            let frame = try await session.wait(Expectation(anyOf: ["/help", "for commands"], timeout: 6))
            try session.save(frame, "03b-placeholder.png")
            return "已截图"
        }

        let themes = ["ink-black", "graphite", "warm-obsidian", "sapphire-night", "moonstone", "deep-petrol"]
        for (index, theme) in themes.enumerated() {
            try await session.step("\(theme)：切换主题并用 /help 重开引导") {
                try await runDockCommand(session, "/theme \(theme)")
                try await Task.sleep(nanoseconds: 700_000_000)
                try await focusDock(session)
                try await session.type("/help")
                _ = try await session.wait(Expectation(anyOf: ["/help", "Help"], timeout: 6))
                // 建议抽屉有展开动效，等落稳后再截图。
                try await Task.sleep(nanoseconds: 900_000_000)
                if index == 0 { try session.save(try await session.capture(), "04-help-suggestion.png") }
                try await session.shortcut(key: 36, modifiers: [])
                // 浅色底上 OCR 会把标题读歪，用多个稳定词任一命中。
                _ = try await session.wait(Expectation(anyOf: ["Feature Guide", "Skip Guide", "Voice Interaction", "Agent Collaboration"], timeout: 8))
                // 引导进场有缩放动效，等落稳后再截图。
                try await Task.sleep(nanoseconds: 900_000_000)
                try session.save(try await session.capture(), "theme-\(theme).png")
                // 第 1 页含安装状态行与「打开设置」主按钮，每个主题各截一次。
                let page = try await session.capture()
                try await session.click(local: CGPoint(x: 215, y: 129), on: page)
                try await Task.sleep(nanoseconds: 700_000_000)
                try session.save(try await session.capture(), "theme-\(theme)-orchestration.png")
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
                return "已截图"
            }
        }

        try await session.step("中文月石主题：九段终版演示可播放与暂停") {
            session.stop()
            try session.launch(language: "zh-CN")
            _ = try await session.wait(Expectation(anyOf: [name, "搜索工作空间"], timeout: 20))
            let home = try await session.capture()
            if home.contains("搜索工作空间") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
            }
            try await runDockCommand(session, "/theme moonstone")
            try await runDockCommand(session, "/help")
            _ = try await session.wait(Expectation(anyOf: ["功能引导", "跳过引导"], timeout: 10))
            for _ in 0..<11 { try await session.shortcut(key: 126, modifiers: []) }
            for page in 2...10 {
                try await session.shortcut(key: 125, modifiers: [])
                _ = try await session.wait(Expectation(allOf: ["播放中"], timeout: 10))
                try await Task.sleep(nanoseconds: 900_000_000)
                try session.save(try await session.capture(), "demo-zh-\(page).png")
            }
            try await session.click(matching: ["暂停演示"], pick: .bottom)
            let paused = try await session.wait(Expectation(allOf: ["已暂停"], timeout: 6))
            try session.save(paused, "demo-zh-paused.png")
            return "中文九段演示、暂停控制已验证"
        }
    }
}

/// 指令输入框在底部 Dock 中偏左（约 42% 宽度，距底 47）。
@MainActor
private func focusDock(_ session: Session) async throws {
    let dock = try await session.capture()
    try await session.click(
        local: CGPoint(x: Double(dock.windowFrame.width) * 0.42, y: Double(dock.windowFrame.height) - 47),
        on: dock
    )
    try await Task.sleep(nanoseconds: 300_000_000)
    // 上一条指令的残留文本会与新输入拼接，先全选再覆盖。
    try await session.selectAll()
}

@MainActor
private func runDockCommand(_ session: Session, _ command: String) async throws {
    try await focusDock(session)
    try await session.type(command)
    try await Task.sleep(nanoseconds: 900_000_000)
    try await session.shortcut(key: 36, modifiers: [])
}

@MainActor
private func openCreateDialog(_ session: Session, entry: [String]) async throws {
    try await session.act("打开创建对话框", attempts: 4, expect: Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3.5)) {
        try await session.click(matching: entry, pick: .bottom)
    }
}

@MainActor
private func fill(_ session: Session, _ name: String) async throws {
    try await session.act("输入 \(name)", attempts: 4, expect: Expectation(allOf: [name], timeout: 3)) {
        let frame = try await session.capture()
        if let placeholder = frame.pick(["例如", "e.g.", "网站改版", "Website redesign"], .bottom) {
            try await session.click(local: placeholder.center, on: frame)
        } else if let label = frame.pick(["工作空间名称", "Workspace name"], .bottom) {
            try await session.click(local: CGPoint(x: label.center.x, y: label.rect.maxY + 32), on: frame)
        } else {
            try await session.click(local: CGPoint(x: frame.windowFrame.width / 2, y: frame.windowFrame.height / 2 - 77), on: frame)
        }
        try await Task.sleep(nanoseconds: 250_000_000)
        try await session.selectAll()
        try await session.type(name)
    }
}

@MainActor
private func confirmCreate(_ session: Session, _ name: String) async throws {
    for attempt in 1...4 {
        if try session.catalog().names.contains(name) { return }
        let frame = try await session.capture()
        if frame.contains("工作空间名称") || frame.contains("Workspace name") {
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
        }
        if (try? await session.waitCatalog(timeout: 4, { $0.names.contains(name) })) != nil { return }
        if attempt == 4 { throw E2EError.product("确认创建后 SQLite 仍没有 \(name)。\(try session.catalog().summary)") }
    }
}
