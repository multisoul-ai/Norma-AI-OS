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
                Expectation(allOf: ["当前项目", "其他项目"], timeout: 6)
            )
            try session.save(overview, "01-drawer-overview.png")
            return overview.summary
        }

        try await session.step("点图钉后概览悬浮到画布右侧") {
            let frame = try await session.capture()
            guard let heading = frame.pick(["Run Overview", "运行概览"], .top) else {
                throw E2EError.product("抽屉概览标题没有出现。可见：\(frame.summary)")
            }
            let drawerRight = min(max(frame.windowFrame.width * 0.31, 366), 438)
            try await session.click(
                local: CGPoint(x: drawerRight - 26, y: heading.center.y),
                on: frame
            )
            try await Task.sleep(nanoseconds: 900_000_000)
            let floating = try await session.capture()
            guard let title = floating.pick(["Run Overview", "运行概览"], .right),
                  title.center.x > floating.windowFrame.width * 0.5 else {
                throw E2EError.product("点图钉后画布上没有出现悬浮概览。可见：\(floating.summary)")
            }
            try session.save(floating, "02-floating-overview.png")
            let unchanged = try session.prove(names: [name], areaCount: 0)
            return unchanged.summary
        }

        try await session.step("拖动悬浮标题后点悬浮面板图钉收回") {
            let frame = try await session.capture()
            guard let title = frame.pick(["Run Overview", "运行概览"], .right) else {
                throw E2EError.product("悬浮概览标题没有出现。可见：\(frame.summary)")
            }
            let destination = CGPoint(x: title.center.x - 80, y: title.center.y + 36)
            try await session.drag(local: title.center, to: destination, on: frame)
            try await Task.sleep(nanoseconds: 700_000_000)
            let moved = try await session.capture()
            guard let movedTitle = moved.pick(["Run Overview", "运行概览"], .right),
                  movedTitle.center.x > moved.windowFrame.width * 0.5 else {
                throw E2EError.product("拖动后悬浮概览标题消失。可见：\(moved.summary)")
            }
            // 悬浮面板宽 360，标题左对齐于面板内边距 16，图钉在面板右侧约 29px 处。
            try await session.click(
                local: CGPoint(x: movedTitle.rect.minX + 315, y: movedTitle.center.y + 10),
                on: moved
            )
            try await Task.sleep(nanoseconds: 900_000_000)
            let docked = try await session.capture()
            if let leftover = docked.pick(["Run Overview", "运行概览"], .right),
               leftover.center.x > docked.windowFrame.width * 0.5 {
                throw E2EError.product("点图钉后悬浮面板没有收回。可见：\(docked.summary)")
            }
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
