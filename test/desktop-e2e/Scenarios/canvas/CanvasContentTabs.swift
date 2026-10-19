import E2EKit
import Fixtures
import Foundation

/// 在真实窗口中验收右侧工作空间标签的恢复、切换和来源移除状态。
@MainActor
public enum CanvasContentTabs {
    public nonisolated static let name = "canvas-content-tabs"

    public static func run(_ session: Session) async throws {
        let workspace = "ContentTabsCheck"
        var projectRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
        }

        try await session.step("建立文件与网页标签夹具") {
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 4))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("创建工作空间时缺少名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            let root = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            projectRoot = root
            try Data("E2E PLAN CONTENT\n".utf8).write(to: root.appendingPathComponent("PlanE2E.md"))
            try Catalog.seedContentTab(
                database: try session.databaseURL(), workspaceName: workspace,
                sourceAreaName: "E2E Project", kind: "file",
                resource: root.appendingPathComponent("PlanE2E.md").path,
                title: "PlanE2E.md", position: 0, selected: true
            )
            try Catalog.seedContentTab(
                database: try session.databaseURL(), workspaceName: workspace,
                sourceAreaName: "E2E Project", kind: "browser",
                resource: "data:text/html,%3Ch1%3EWEB%20PAGE%20E2E%3C%2Fh1%3E", title: "WebE2E",
                position: 1, selected: false
            )
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await openWorkspace(session)
            let catalog = try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace)
            return catalog.summary
        }

        try await session.step("恢复文件标签及预览内容") {
            try await openRightDrawer(session)
            let frame = try await session.wait(Expectation(allOf: ["PlanE2E", "WebE2E", "E2E PLAN CONTENT"], timeout: 12))
            let tabs = try session.contentTabs(workspaceName: workspace)
            guard tabs.count == 2,
                  tabs.first(where: { $0.title == "PlanE2E.md" })?.selected == true,
                  tabs.first(where: { $0.title == "WebE2E" })?.selected == false else {
                throw E2EError.product("文件标签恢复或选中项不正确：\(tabs)")
            }
            try session.save(frame, "01-file-restored.png")
            return "右栏恢复两个标签，文件正文按需加载"
        }

        try await session.step("切换网页与文件标签") {
            try await session.click(matching: ["WebE2E"], pick: .top)
            let web = try await session.wait(Expectation(allOf: ["WebE2E", "输入网页地址", "E2E Project", "WEB PAGE E2E"], timeout: 8))
            guard try session.contentTabs(workspaceName: workspace).first(where: { $0.title == "WebE2E" })?.selected == true else {
                throw E2EError.product("点击网页标签后 SQLite 未保存选中项")
            }
            try session.save(web, "02-browser-selected.png")
            try await session.click(matching: ["PlanE2E.md"], pick: .top)
            let file = try await session.wait(Expectation(allOf: ["E2E PLAN CONTENT"], absent: ["输入网页地址"], timeout: 8))
            guard try session.contentTabs(workspaceName: workspace).first(where: { $0.title == "PlanE2E.md" })?.selected == true else {
                throw E2EError.product("切回文件标签后 SQLite 未保存选中项")
            }
            try session.save(file, "03-file-reselected.png")
            return "网页和文件在同一工作空间标签组里切换"
        }

        try await session.step("收起右栏再打开保留选中标签") {
            let before = try await session.capture()
            let width = Double(before.windowFrame.width)
            let rightWidth = min(560, max(370, width * 0.37))
            try await session.click(local: CGPoint(x: width - rightWidth + 28, y: 26), on: before)
            let closed = try await session.wait(Expectation(absent: ["PlanE2E.md", "WebE2E", "E2E PLAN CONTENT"], timeout: 8))
            try session.save(closed, "04a-drawer-closed.png")
            try await openRightDrawer(session)
            let restored = try await session.wait(Expectation(allOf: ["PlanE2E", "WebE2E", "E2E PLAN CONTENT"], timeout: 8))
            try session.save(restored, "04-drawer-reopened.png")
            return "收起抽屉只隐藏视图，不结束文件标签"
        }

        try await session.step("重启后恢复标签列表和选中项") {
            session.stop()
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await openWorkspace(session)
            try await openRightDrawer(session)
            let restored = try await session.wait(Expectation(allOf: ["PlanE2E", "WebE2E", "E2E PLAN CONTENT"], timeout: 12))
            guard try session.contentTabs(workspaceName: workspace).first(where: { $0.title == "PlanE2E.md" })?.selected == true else {
                throw E2EError.product("重启后文件标签不再是选中项")
            }
            try session.save(restored, "05-after-restart.png")
            return "App 重启后从持久化记录恢复两个标签及选中项"
        }

        try await session.step("来源项目移除后标签仍能查看") {
            let before = try await session.capture()
            let width = Double(before.windowFrame.width)
            let rightWidth = min(560, max(370, width * 0.37))
            guard let project = before.texts.first(where: {
                $0.value.contains("E2E Project") && $0.center.x > 300 && $0.center.x < width - rightWidth
            }) else {
                throw E2EError.product("移除前找不到中央项目卡片：\(before.summary)")
            }
            try await session.click(local: CGPoint(x: width - rightWidth - 44, y: project.center.y), on: before)
            _ = try await session.wait(Expectation(anyOf: ["移除", "Remove"], timeout: 4))
            try await session.click(matching: ["移除", "Remove"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["移除项目区", "Remove Project Area"], timeout: 4))
            try await session.click(matching: ["删除", "Delete"], pick: .bottom)
            _ = try await session.waitCatalog(timeout: 8) { $0.areas.isEmpty }
            let removed = try await session.wait(Expectation(allOf: ["来源项目已移除", "E2E PLAN CONTENT"], timeout: 12))
            guard try session.contentTabs(workspaceName: workspace).count == 2,
                  let projectRoot,
                  FileManager.default.fileExists(atPath: projectRoot.appendingPathComponent("PlanE2E.md").path) else {
                throw E2EError.product("移除来源项目后标签或原文件丢失")
            }
            try session.save(removed, "06-source-removed.png")
            return "来源已移除标记可见，标签及原文件保留"
        }

        try await session.step("关闭文件标签不删除文件") {
            try await closeTab(session, title: "PlanE2E.md")
            let after = try await session.wait(Expectation(allOf: ["WebE2E"], absent: ["PlanE2E.md"], timeout: 8))
            guard try session.contentTabs(workspaceName: workspace).map(\.title) == ["WebE2E"],
                  let projectRoot,
                  FileManager.default.fileExists(atPath: projectRoot.appendingPathComponent("PlanE2E.md").path) else {
                throw E2EError.product("关闭文件标签后数据库或原文件状态错误")
            }
            try session.save(after, "07-file-closed.png")
            return "文件标签记录已删除，磁盘文件保持原样"
        }
    }
}

@MainActor
private func openWorkspace(_ session: Session) async throws {
    let launch = try await session.capture()
    if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
        try await session.click(matching: ["打开", "Open"], pick: .bottom)
    }
    _ = try await session.wait(Expectation(allOf: ["当前对话"], timeout: 12))
}

@MainActor
private func openRightDrawer(_ session: Session) async throws {
    let frame = try await session.capture()
    if frame.contains("PlanE2E.md") && frame.contains("WebE2E") { return }
    try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 28, y: 26), on: frame)
    _ = try await session.wait(Expectation(allOf: ["PlanE2E", "WebE2E"], timeout: 8))
}

@MainActor
private func closeTab(_ session: Session, title: String) async throws {
    let frame = try await session.capture()
    guard let label = frame.exact(title) else {
        throw E2EError.product("找不到要关闭的标签：\(title)。可见：\(frame.summary)")
    }
    let close = frame.texts
        .filter { $0.value == "×" && $0.center.x > label.center.x && abs($0.center.y - label.center.y) < 14 }
        .min { $0.center.x < $1.center.x }
    let x = close?.center.x ?? label.rect.maxX + 19
    try await session.click(local: CGPoint(x: x, y: label.center.y), on: frame)
}
