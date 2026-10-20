import E2EKit
import Foundation

@MainActor
public enum WorkspaceStatusCapsule {
    public nonisolated static let name = "workspace-status-capsule"

    public static func run(_ session: Session) async throws {
        let name = "StatusCapsule"
        defer { session.stop() }

        try await session.step("创建空工作空间") {
            try session.launch()
            try await openCreateDialog(session, entry: ["创建工作空间", "Create Workspace"])
            try await fill(session, name)
            try await confirmCreate(session, name)
            _ = try await session.wait(Expectation(allOf: [name], timeout: 12))
            let catalog = try session.prove(names: [name], areaCount: 0)
            return catalog.summary
        }

        try await session.step("空工作空间不显示状态胶囊") {
            let canvas = try await session.wait(
                Expectation(
                    allOf: [name],
                    absent: ["待处理", "运行中 4/6", "待开始"],
                    timeout: 6
                )
            )
            try session.save(canvas, "01-empty-capsule.png")
            let unchanged = try session.prove(names: [name], areaCount: 0)
            return unchanged.summary
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
