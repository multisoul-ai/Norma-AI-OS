import E2EKit
import Foundation

@MainActor
public enum WorkspaceOverviewFloat {
    public nonisolated static let name = "workspace-overview-float"

    public static func run(_ session: Session) async throws {
        let name = "FloatOverview"
        defer { session.stop() }

        try await session.step("创建工作空间并打开运行概览") {
            try session.launch()
            try await openCreateDialog(session, entry: ["创建工作空间", "Create Workspace"])
            try await fill(session, name)
            try await confirmCreate(session, name)
            _ = try await session.wait(Expectation(allOf: [name], timeout: 12))
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            let overview = try await session.wait(
                Expectation(allOf: ["对话", "运行概览"], timeout: 6)
            )
            try session.save(overview, "01-drawer-overview.png")
            return overview.summary
        }

        try await session.step("点图钉后概览悬浮，左栏回到对话") {
            let frame = try await session.capture()
            guard let title = frame.pick(["运行概览"], .bottom) else {
                throw E2EError.product("抽屉概览标题没有出现。可见：\(frame.summary)")
            }
            let drawerRight = min(max(frame.windowFrame.width * 0.31, 366), 438)
            try await session.click(
                local: CGPoint(x: drawerRight - 26, y: title.center.y),
                on: frame
            )
            let floating = try await session.wait(
                Expectation(allOf: ["和 Norma 聊聊", "运行概览"], timeout: 6)
            )
            try session.save(floating, "02-floating-overview.png")
            let unchanged = try session.prove(names: [name], areaCount: 0)
            return unchanged.summary
        }

        try await session.step("拖动悬浮标题后点运行概览标签收回") {
            let frame = try await session.capture()
            guard let title = frame.pick(["运行概览"], .right) else {
                throw E2EError.product("悬浮概览标题没有出现。可见：\(frame.summary)")
            }
            let destination = CGPoint(x: title.center.x - 80, y: title.center.y + 36)
            try await session.drag(local: title.center, to: destination, on: frame)
            try await session.click(matching: ["运行概览"], pick: .top)
            let docked = try await session.wait(
                Expectation(
                    allOf: ["运行概览", "需要你处理"],
                    absent: ["和 Norma 聊聊"],
                    timeout: 6
                )
            )
            try session.save(docked, "03-docked-overview.png")
            return docked.summary
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
