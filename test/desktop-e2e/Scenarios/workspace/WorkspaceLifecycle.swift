import E2EKit
import Fixtures
import Foundation

@MainActor
public enum WorkspaceLifecycle {
    public nonisolated static let name = "workspace-lifecycle"

    public static func run(_ session: Session) async throws {
        let alpha = "AlphaKeep"
        let beta = "BetaKeep"
        let gamma = "GammaKeep"
        defer { session.stop() }

        try await session.step("前置检查") {
            "辅助功能、屏幕录制、未锁屏已在启动前完成；用户已有 Norma 不会被结束"
        }

        try await session.step("创建工作空间") {
            try session.launch()
            let welcome = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try session.save(welcome, "01-welcome.png")
            try await openCreateDialog(session, entry: ["创建工作空间", "Create Workspace"])
            let dialog = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3))
            try session.save(dialog, "02-create-dialog.png")
            try await fill(session, alpha)
            let named = try await session.wait(Expectation(allOf: [alpha], timeout: 3))
            try session.save(named, "03-name-entered.png")
            try await confirmCreate(session, alpha)
            _ = try await session.wait(Expectation(allOf: [alpha], timeout: 12))
            let catalog = try session.prove(names: [alpha], areaCount: 0)
            let created = try await session.capture()
            try session.save(created, "04-created.png")
            return catalog.summary
        }

        try await session.step("目录列出两个工作空间") {
            try await showDirectory(session, required: [alpha])
            try await openCreateDialog(session, entry: ["新建工作空间", "New Workspace"])
            try await fill(session, beta)
            try await confirmCreate(session, beta)
            // 创建完成会打开新工作空间的画布，目录名单要回到目录后再看。
            try await showDirectory(session, required: [alpha, beta])
            let listed = try await session.wait(Expectation(anyOf: ["继续工作", "Continue", "打开", "Open"], allOf: [alpha, beta], timeout: 8))
            try session.save(listed, "05-directory-both.png")
            let catalog = try session.prove(names: [alpha, beta], areaCount: 0)
            return catalog.summary
        }

        try await session.step("搜索按名称过滤") {
            try await session.act("输入搜索", attempts: 4, expect: Expectation(allOf: [alpha], absent: [beta], timeout: 4)) {
                try await focusSearch(session, queryHint: "搜索工作空间")
                try await session.selectAll()
                try await session.type("Alpha")
            }
            let filtered = try await session.wait(Expectation(allOf: [alpha], absent: [beta], timeout: 3))
            try session.save(filtered, "06-search-filtered.png")
            let unchanged = try session.prove(names: [alpha, beta], areaCount: 0)
            try await session.act("清空搜索", attempts: 4, expect: Expectation(allOf: [alpha, beta], timeout: 4)) {
                try await focusSearch(session, queryHint: "Alpha")
                try await session.selectAll()
                try await session.deleteSelection()
            }
            let restored = try await session.wait(Expectation(allOf: [alpha, beta], timeout: 3))
            try session.save(restored, "07-search-cleared.png")
            return unchanged.summary
        }

        var sentinel = ""
        let seeded = try await session.step("写入项目区夹具") {
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("数据目录丢失") }
            let sentinelURL = try ProjectAreaFixture.install(dataRoot: dataRoot, workspaceName: alpha)
            sentinel = sentinelURL.path
            try session.launch()
            try await showDirectory(session, required: [alpha, beta])
            let catalog = try session.prove(names: [alpha, beta], areaCount: 1, linkedTo: alpha)
            let frame = try await session.capture()
            try session.save(frame, "08-directory-with-project.png")
            return "\(catalog.summary) sentinel=\(sentinel)"
        }
        _ = seeded

        try await session.step("重命名工作空间") {
            try await openMenu(session)
            let menu = try await session.wait(Expectation(anyOf: ["重命名", "Rename"], timeout: 3))
            try session.save(menu, "09-actions-menu.png")
            try await session.act("打开重命名", attempts: 3, expect: Expectation(anyOf: ["重命名工作空间", "Rename Workspace"], timeout: 4)) {
                try await session.click(matching: ["重命名", "Rename"], pick: .bottom)
            }
            let dialog = try await session.wait(Expectation(anyOf: ["重命名工作空间", "Rename Workspace"], timeout: 3))
            try session.save(dialog, "10-rename-dialog.png")
            try await session.act("输入新名称", attempts: 4, expect: Expectation(allOf: [gamma], absent: ["AlphaKeepGammaKeep", "18/80"], timeout: 3)) {
                let frame = try await session.capture()
                if let title = frame.pick(["重命名工作空间", "Rename Workspace"], .top) {
                    try await session.click(local: CGPoint(x: title.center.x, y: title.rect.maxY + 36), on: frame)
                }
                try await session.selectAll()
                try await session.type(gamma)
            }
            try await confirmRename(session, from: alpha, to: gamma)
            let renamed = try await session.wait(Expectation(allOf: [gamma, beta], absent: ["重命名工作空间", "Rename Workspace", alpha], timeout: 8))
            try session.save(renamed, "11-renamed.png")
            let catalog = try session.prove(names: [gamma, beta], areaCount: 1, linkedTo: gamma)
            return catalog.summary
        }

        try await session.step("取消删除") {
            try await openMenu(session)
            try await session.act("打开删除确认", attempts: 3, expect: Expectation(anyOf: ["删除工作空间", "Delete Workspace"], timeout: 4)) {
                try await clickMenuDelete(session)
            }
            let confirmation = try await session.wait(Expectation(anyOf: ["删除工作空间", "Delete Workspace"], allOf: [gamma], timeout: 3))
            try session.save(confirmation, "12-delete-confirmation.png")
            try await session.act("点击取消", attempts: 3, expect: Expectation(allOf: [gamma, beta], absent: ["删除工作空间", "Delete Workspace"], timeout: 4)) {
                try await session.click(matching: ["取消", "Cancel"], pick: .bottom)
            }
            let canceled = try await session.capture()
            try session.save(canceled, "13-delete-canceled.png")
            let catalog = try session.prove(names: [gamma, beta], areaCount: 1, linkedTo: gamma)
            return catalog.summary
        }

        try await session.step("确认删除并保留项目目录") {
            try await openMenu(session)
            try await session.act("再次打开删除确认", attempts: 3, expect: Expectation(anyOf: ["删除工作空间", "Delete Workspace"], timeout: 4)) {
                try await clickMenuDelete(session)
            }
            try await confirmDelete(session, gone: gamma, remaining: [beta])
            guard FileManager.default.fileExists(atPath: sentinel) else {
                throw E2EError.product("删除工作空间后 keep-me.txt 不应消失：\(sentinel)")
            }
            let deleted = try await session.wait(Expectation(allOf: [beta], absent: [gamma, "删除工作空间", "Delete Workspace"], timeout: 8))
            try session.save(deleted, "14-deleted.png")
            let catalog = try session.prove(names: [beta], areaCount: 0)
            return "\(catalog.summary) keep-me=\(sentinel)"
        }

        try await session.step("重启后不恢复已删除工作空间") {
            session.stop()
            try session.launch()
            try await showDirectory(session, required: [beta])
            let restarted = try await session.wait(Expectation(allOf: [beta], absent: [gamma, alpha], timeout: 8))
            try session.save(restarted, "15-after-restart.png")
            guard FileManager.default.fileExists(atPath: sentinel) else {
                throw E2EError.product("重启后 keep-me.txt 必须仍在：\(sentinel)")
            }
            let catalog = try session.prove(names: [beta], areaCount: 0)
            return catalog.summary
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

@MainActor
private func showDirectory(_ session: Session, required: [String]) async throws {
    try await session.act("打开工作空间目录", attempts: 3, expect: Expectation(anyOf: ["打开", "Open", "新建工作空间", "New Workspace", "工作空间"], allOf: required, timeout: 6)) {
        try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
    }
}

@MainActor
private func focusSearch(_ session: Session, queryHint: String) async throws {
    let frame = try await session.capture()
    if let field = frame.texts.filter({ $0.value.localizedCaseInsensitiveContains(queryHint) && $0.center.y < 64 }).min(by: { $0.center.y < $1.center.y }) {
        try await session.click(local: field.center, on: frame)
        return
    }
    if let field = frame.pick(["搜索工作空间", "Search workspaces"], .top) {
        try await session.click(local: field.center, on: frame)
        return
    }
    if let button = frame.pick(["新建工作空间", "New Workspace"], .top) {
        try await session.click(local: CGPoint(x: button.rect.minX - 130, y: button.center.y), on: frame)
        return
    }
    throw E2EError.product("找不到搜索框。可见：\(frame.summary)")
}

@MainActor
private func openMenu(_ session: Session) async throws {
    try await session.act("打开操作菜单", attempts: 4, expect: Expectation(anyOf: ["重命名", "Rename", "删除", "Delete"], absent: ["删除工作空间", "Delete Workspace", "重命名工作空间", "Rename Workspace"], timeout: 3)) {
        try await session.openFeaturedActions()
    }
}

@MainActor
private func confirmRename(_ session: Session, from old: String, to new: String) async throws {
    for attempt in 1...4 {
        let catalog = try session.catalog()
        if catalog.names.contains(new) && !catalog.names.contains(old) { return }
        let frame = try await session.capture()
        if frame.contains("重命名工作空间") || frame.contains("Rename Workspace") {
            try await session.click(matching: ["保存", "Save"], pick: .bottom)
        }
        let ready: (Catalog) -> Bool = { $0.names.contains(new) && !$0.names.contains(old) }
        if (try? await session.waitCatalog(timeout: 4, ready)) != nil { return }
        if attempt == 4 { throw E2EError.product("重命名后 SQLite 未变成 \(new)。\(try session.catalog().summary)") }
    }
}

/// 只点操作菜单里、紧挨在「重命名」下面的「删除」。
/// 确认框标题也包含「删除」，重试时不能改点底部的确认按钮。
@MainActor
private func clickMenuDelete(_ session: Session) async throws {
    let frame = try await session.capture()
    if frame.contains("删除工作空间") || frame.contains("Delete Workspace") { return }
    guard let rename = frame.exact("重命名") ?? frame.exact("Rename"),
          let item = frame.exact("删除") ?? frame.exact("Delete") else {
        throw E2EError.product("菜单里没有删除。可见：\(frame.summary)")
    }
    let belowRename = item.center.y > rename.center.y
        && item.center.y - rename.center.y < 80
        && abs(item.center.x - rename.center.x) < 80
    guard belowRename else {
        throw E2EError.product("删除不在操作菜单里。可见：\(frame.summary)")
    }
    try await session.click(local: item.center, on: frame)
}

@MainActor
private func confirmDelete(_ session: Session, gone: String, remaining: [String]) async throws {
    for attempt in 1...4 {
        let catalog = try session.catalog()
        if !catalog.names.contains(gone) && Set(catalog.names) == Set(remaining) { return }
        let frame = try await session.capture()
        if frame.contains("删除工作空间") || frame.contains("Delete Workspace") {
            try await session.click(matching: ["删除", "Delete"], pick: .bottom)
        }
        let ready: (Catalog) -> Bool = { !$0.names.contains(gone) && Set($0.names) == Set(remaining) && $0.areas.isEmpty }
        if (try? await session.waitCatalog(timeout: 4, ready)) != nil { return }
        if attempt == 4 { throw E2EError.product("确认删除后 SQLite 仍保留 \(gone)。\(try session.catalog().summary)") }
    }
}
