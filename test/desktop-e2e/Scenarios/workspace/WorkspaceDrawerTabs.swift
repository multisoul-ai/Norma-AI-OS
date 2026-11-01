import E2EKit
import Fixtures
import Foundation

@MainActor
public enum WorkspaceDrawerTabs {
    public nonisolated static let name = "workspace-drawer-tabs"

    public static func run(_ session: Session) async throws {
        if session.visualReview { return try await runVisual(session) }
        let name = "DrawerTabs"
        defer { session.stop() }

        try await session.step("创建工作空间") {
            try session.launch()
            try await openCreateDialog(session, entry: ["创建工作空间", "Create Workspace"])
            try await fill(session, name)
            try await confirmCreate(session, name)
            _ = try await session.wait(Expectation(allOf: [name], timeout: 12))
            let catalog = try session.prove(names: [name], areaCount: 0)
            return catalog.summary
        }

        try await session.step("⌘⇧E 展开左栏并进入运行概览") {
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            let overview = try await session.wait(
                Expectation(allOf: ["运行中", "待开始"], timeout: 6)
            )
            try session.save(overview, "01-run-overview.png")
            let unchanged = try session.prove(names: [name], areaCount: 0)
            return unchanged.summary
        }

        try await session.step("再次 ⌘⇧E 仍是运行概览，Chat 标签与对话输入保持隐藏") {
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            let overview = try await session.wait(
                Expectation(
                    allOf: ["运行中", "待开始"],
                    absent: ["Chat", "和 Norma 聊聊"],
                    timeout: 6
                )
            )
            // 画布底部 Dock 也有同名占位符；只判左栏范围内不应有对话输入框。
            let frame = try await session.capture()
            if let input = frame.pick(["Talk to Norma", "和 Norma 聊聊"], .left),
               input.center.x < frame.windowFrame.width * 0.3 {
                throw E2EError.product("左栏不应出现对话输入框。可见：\(frame.summary)")
            }
            try session.save(overview, "02-chat-hidden.png")
            let unchanged = try session.prove(names: [name], areaCount: 0)
            return unchanged.summary
        }
    }

    private static func runVisual(_ session: Session) async throws {
        let name = "DrawerTabs"
        let idea = "悬停查看完整想法：左侧栏宽度缩小后保留完整项目与任务上下文，第二行的内容也必须显示出来。\n隐藏末行：Tooltip 完整正文验收完成。"
        defer { session.stop() }
        var database: URL?
        try await session.step("建立隔离工作空间") {
            try session.launch(language: "zh-Hans")
            let db = try session.databaseURL()
            database = db
            _ = try await session.waitCatalog(timeout: 20) { _ in
                FileManager.default.fileExists(atPath: db.path)
            }
            session.stop()
            try Catalog.seedWorkspace(database: db, name: name)
            try Catalog.suppressFeatureGuide(database: db)
            try Catalog.seedIdea(database: db, workspaceName: name, body: idea, position: 1024)
            return try session.prove(names: [name], areaCount: 0).summary
        }
        guard let database else { throw E2EError.env("缺少隔离数据库") }
        for (language, theme) in [("zh-Hans", "sapphire-night"), ("en", "paper-blue")] {
            try await session.step("\(theme)：启动真实窗口") {
                session.stop()
                try VoiceConfigurationFixture.theme(database: database, id: theme)
                try session.launch(language: language)
                try await session.resize(width: 1280, height: 800)
                return "1280×800，\(language)"
            }
            try await session.step("⌘⇧E 展开缩窄后的左栏") {
                try await session.visualClick("点击最近访问区域已有工作空间卡片底部的 DrawerTabs 名称文字，进入工作空间；不要点击预览中的加号")
                _ = try await session.visualCheck([
                    VisualCriterion(id: "entered", text: "已离开工作空间目录，顶部出现 DrawerTabs / 所有项目，中央为承载项目的画布或 Project canvas；若过渡尚未结束则等待")
                ])
                try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
                let overview = try await session.visualCheck([
                    VisualCriterion(id: "width", text: "1280 点宽窗口中，左侧栏右边缘约在 x=247 点（原 396.8 减 150），中央画布从该边缘开始，互不重叠"),
                    VisualCriterion(id: "overview", text: "左栏显示运行概览的运行中和待开始分区，标题及按钮可读，没有横向溢出或互相遮挡")
                ])
                try session.save(overview, "\(theme)/01-run-overview.png")
                return try session.prove(names: [name], areaCount: 0).summary
            }
            try await session.step("再次 ⌘⇧E 保持运行概览与紧凑宽度") {
                try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
                let overview = try await session.visualCheck([
                    VisualCriterion(id: "overview", text: "左栏保持约 247 点宽，运行概览仍然可见；没有 Chat 标签，左栏内没有对话输入框；中央底部 Dock 可以保留")
                ])
                try session.save(overview, "\(theme)/02-chat-hidden.png")
                return try session.prove(names: [name], areaCount: 0).summary
            }
            try await session.step("\(theme)：悬停想法显示完整正文") {
                let (frame, point) = try await session.visualLocate("定位左侧想法栏中以「悬停查看完整想法」开头的想法标题文字；不要定位指派或删除按钮")
                try await session.hover(local: point, on: frame)
                let tooltip = try await session.visualCheck([
                    VisualCriterion(id: "tooltip", text: "悬停提示中可读到完整想法正文，包括原列表隐藏的末行「隐藏末行：Tooltip 完整正文验收完成。」；文本自动换行，没有被左栏边缘裁切"),
                    VisualCriterion(id: "contrast", text: "提示文字与背景对比清晰，提示出现没有改变左栏约 247 点的宽度")
                ])
                try session.save(tooltip, "\(theme)/03-idea-tooltip.png")
                try await session.hover(local: CGPoint(x: 1100, y: 400), on: tooltip)
                let dismissed = try await session.visualCheck([
                    VisualCriterion(id: "dismissed", text: "鼠标移到中央画布后，完整正文提示框消失，隐藏末行不再可见；想法行仍在列表内")
                ])
                try session.save(dismissed, "\(theme)/04-tooltip-dismissed.png")
                return "完整正文可悬停查看，移出后消失"
            }
        }
    }
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
