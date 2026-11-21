import CoreGraphics
import E2EKit
import Fixtures
import Foundation

@MainActor
extension ContentTabControls {
    // The spare tab exercises middle click before the three-tab menu sequence.
    static func closeTabs(_ session: Session, fixture: ContentTabControlsFixture,
                          server: ContentBrowserFixture, resources initialResources: [String]) async throws {
        let resources = initialResources.filter { $0 != fixture.middle.path }
        try await session.step("中键拖入不关闭标签，完整中键点击只关闭目标") {
            let spare = try await session.visualLocate("定位右抽屉标签条第三个 Spare.txt 胶囊的可见文字中心")
            let outside = CGPoint(x: spare.point.x, y: spare.point.y + 42)
            try await session.drag(local: outside, to: spare.point, on: spare.frame, button: .middle)
            try await check(session, "00a-middle-drag-en.png", VisualCriterion(id: "drag", text: "从标签条下方按中键、移入 Spare.txt 再松开后，Spare.txt 仍在标签条，First.txt 仍选中且显示 FILE BEFORE。没有误出现菜单。"))
            _ = try await waitTabs(session, order: initialResources, selected: fixture.start.path)
            let target = try await session.visualLocate("定位仍在右抽屉标签条上的 Spare.txt 胶囊文字中心")
            try await session.click(local: target.point, on: target.frame, button: .middle)
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            try await check(session, "00b-middle-closed-en.png", VisualCriterion(id: "click", text: "中键完整点击后仅 Spare.txt 从标签条消失，First.txt 仍选中，显示 FILE BEFORE。"))
            guard try String(contentsOf: fixture.middle, encoding: .utf8) == "SPARE FILE\n" else {
                throw E2EError.product("中键关闭标签改变了实际文件")
            }
            return "移入松开保留全部标签，完整中键点击仅移除 Spare.txt 并保留实际文件"
        }
        try await refreshBrowser(session, fixture: fixture, server: server, resources: resources, english: true)
        let pair = [fixture.start.path, fixture.zulu.path]
        let single = [fixture.zulu.path]
        try await session.step("末位菜单禁用右侧关闭，右键及 Shift+F10 定位未选中标签") {
            try await actions(session)
            try await check(session, "05-last-menu-en.png", VisualCriterion(id: "menu", text: "蓝宝石之夜中，网页上方完整显示 Loopback 标题及英文关闭、关闭其他标签、关闭右侧标签、关闭所有标签、分隔线和重新加载，顺序正确。仅关闭右侧一行禁用，其余明晰可辨。"))
            try await session.shortcut(key: 53, modifiers: [])
            let prior = try await waitTabs(session, order: resources, selected: server.refreshAddress)
            let other = try await session.visualLocate("定位右抽屉标签条未选中的 Plan.txt 胶囊文字中心，用于右键操作")
            try await session.click(local: other.point, on: other.frame, button: .right)
            try await check(session, "05a-right-click-en.png", VisualCriterion(id: "right-click", text: "在未选中的 Plan.txt 上右键后，操作菜单标题为 Plan.txt，关闭右侧可用；Loopback 胶囊及网页上下文仍保持选中。"))
            let rightClicked = try await waitTabs(session, order: resources, selected: server.refreshAddress)
            guard rightClicked == prior else { throw E2EError.product("右键打开菜单改变了原标签或选中项") }
            try await session.shortcut(key: 53, modifiers: [])
            try await session.visualClick("点击标签条中的 First.txt 胶囊中央，避开关闭 ×，以选中并聚焦它")
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            try await session.shortcut(key: 48, modifiers: []) // Focus Plan.txt without selecting it.
            try await session.shortcut(key: 109, modifiers: [0x38]) // Shift+F10
            try await check(session, "06-unselected-menu-en.png", VisualCriterion(id: "target", text: "键盘打开的菜单标题为 Plan.txt，关闭右侧标签可用。背景 First.txt 仍选中且显示 FILE BEFORE，键盘焦点与选中是两种状态。"))
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            try await menuItem(session, "Close Tabs to the Right")
            _ = try await waitTabs(session, order: pair, selected: fixture.start.path)
            try await check(session, "07-right-closed-en.png", VisualCriterion(id: "right", text: "菜单收起，First.txt 和 Plan.txt 两个胶囊仍可见，Loopback 从标签条消失。First.txt 与 FILE BEFORE 保持当前。"))
            return "右侧关闭仅移除 Plan.txt 右边的 Loopback，未移除目标或改变可保留的选中项"
        }
        try await session.step("关闭当前标签后选中右邻，单标签禁用其他及右侧操作") {
            try await actions(session)
            try await menuItem(session, "Close（仅关闭当前标签的第一项）")
            _ = try await waitTabs(session, order: single, selected: fixture.zulu.path)
            try await actions(session)
            try await check(session, "08-single-menu-en.png", VisualCriterion(id: "single", text: "仅有 Plan.txt 标签，正文是真实 ZULU FILE。菜单标题为 Plan.txt，Close Other Tabs 与 Close Tabs to the Right 显示禁用样式，Close、关闭全部和 Reload 仍可用。"))
            try await session.shortcut(key: 53, modifiers: [])
            guard FileManager.default.fileExists(atPath: fixture.start.path) else { throw E2EError.product("关闭文件标签删除了实际文件") }
            return "第一项被精确移除，右邻文件显示，原文件仍在磁盘"
        }
        try await session.step("通过新建网页恢复标签，再用关闭其他保留当前文件") {
            try await session.visualClick("点击右抽屉标签条最右侧新建网页的加号，避开搜索和刷新")
            try await session.visualClick("点击新标签首页的 Search or enter a URL 地址输入框")
            try await session.type(server.address)
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await waitTabs(session, order: single + [server.address], selected: server.address)
            try await check(session, "09-browser-recovered-en.png", VisualCriterion(id: "reopen", text: "已有 Plan.txt 保留，新建网页选中并显示 BROWSER FIRST E2E。右端四处操作及标题没有溢出。"))
            try await session.visualClick("点击右抽屉标签条中的 Plan.txt 胶囊中心以切回文件，避开 ×")
            _ = try await waitTabs(session, order: single + [server.address], selected: fixture.zulu.path)
            try await actions(session)
            try await menuItem(session, "Close Other Tabs")
            _ = try await waitTabs(session, order: single, selected: fixture.zulu.path)
            return "真实网页重新打开，关闭其他只保留已选的 Plan.txt"
        }
        try await session.step("重启只恢复未关闭项") {
            session.stop()
            try await openDrawer(session, language: "en")
            _ = try await waitTabs(session, order: single, selected: fixture.zulu.path)
            try await check(session, "10-resumed-en.png", VisualCriterion(id: "resumed", text: "真实 App 重启后，唯一恢复且选中的标签是 Plan.txt，右侧正文为 ZULU FILE。先前已关闭的文件或浏览器标签未复活。"))
            return "只恢复原来存活的 Plan.txt 标签"
        }
        try await closeWithWorkbenchDraft(session, fixture: fixture)
        try await session.step("关闭全部后重启仍为空，首页可恢复工作台") {
            session.stop()
            try await openDrawer(session, language: "en")
            _ = try await waitTabs(session, order: [], selected: nil)
            try await check(session, "12-empty-restored-en.png", VisualCriterion(id: "restart", text: "重启后右抽屉依然是完整首页，不再显示 Files、Plan.txt 或 Loopback 标签。"))
            try await openFilesTool(session)
            let rows = try await waitTabs(session, order: ["project-files"], selected: "project-files")
            guard rows.first?.kind == "project_files" else {
                throw E2EError.product("首页 Files 未新建并选中唯一的工作台标签：\(rows)")
            }
            try await check(session, "13-project-recovered-en.png", VisualCriterion(id: "files", text: "从首页 Files 重新进入真实项目文件工作台，显示 All files 文件区，工作台头部仍有 ⋯ 标签操作入口。"))
            return "批量关闭持久化为空，首页可打开项目工作台"
        }
    }

    private static func closeWithWorkbenchDraft(_ session: Session, fixture: ContentTabControlsFixture) async throws {
        try await session.step("批量关闭时取消或保存冲突均保持草稿和整批标签") {
            try await session.visualClick("点击右抽屉标签条的新建网页加号，打开工具首页")
            try await openFilesTool(session)
            let order = [fixture.zulu.path, "project-files"]
            let original = try await waitTabs(session, order: order, selected: "project-files")
            try await session.visualClick("展开项目文件工作台目录中的 All files 分区")
            try await session.visualClick("点击工作台文件树里的 First.txt 文件行，打开实际文件")
            try await session.visualClick("点击 First.txt 路径栏同一行的铅笔 ✎ 编辑图标，避开复制与系统打开按钮")
            // Editing the file focuses the real code editor, as in ContentWorkbench.
            try await session.selectAll()
            try await session.type("UNSAVED DRAFT E2E")
            try await actions(session)
            try await menuItem(session, "Close All Tabs")
            try await session.visualClick("点击未保存更改提示中的 Cancel，取消本次批量关闭")
            try await check(session, "10a-draft-cancelled-en.png", VisualCriterion(id: "cancel", text: "未保存提示收起，Files 工作台和 Plan.txt 标签均可见且 Files 仍选中。编辑器仍显示 UNSAVED DRAFT E2E，路径栏有未保存标记。"))
            let cancelled = try await waitTabs(session, order: order, selected: "project-files")
            guard cancelled == original, try String(contentsOf: fixture.start, encoding: .utf8) == "FILE BEFORE\n" else {
                throw E2EError.product("取消关闭后标签身份、选中或磁盘内容改变")
            }

            try await actions(session)
            try await menuItem(session, "Close All Tabs")
            // Use a distinct on-disk version after entering the real confirmation, then attempt Save all.
            try fixture.write("EXTERNAL DISK VERSION E2E", to: fixture.start)
            try await session.visualClick("点击未保存更改提示中的 Save all")
            try await check(session, "10b-draft-conflict-en.png", VisualCriterion(id: "conflict", text: "关闭确认仍显示 Save all、Discard drafts and continue、Cancel，以及文件已在外部修改、草稿已保留的英文错误。Files 和 Plan.txt 仍在标签条，抽屉没有返回首页。"))
            let conflicted = try await waitTabs(session, order: order, selected: "project-files")
            guard conflicted == original, try String(contentsOf: fixture.start, encoding: .utf8) == "EXTERNAL DISK VERSION E2E\n" else {
                throw E2EError.product("保存冲突错误地关闭了标签或覆盖了外部文件")
            }
            try await session.visualClick("点击冲突确认中的 Cancel，返回原工作台草稿")
            try await check(session, "10c-draft-kept-en.png", VisualCriterion(id: "draft", text: "工作台草稿仍显示 UNSAVED DRAFT E2E；编辑器未被磁盘上的 EXTERNAL DISK VERSION E2E 替换，Plan.txt 与选中的 Files 标签都保留。"))

            try await actions(session)
            try await menuItem(session, "Close All Tabs")
            try await session.visualClick("点击未保存更改提示中的 Discard drafts and continue，明确放弃草稿")
            _ = try await waitTabs(session, order: [], selected: nil)
            try await check(session, "11-all-closed-en.png", VisualCriterion(id: "home", text: "明确放弃草稿并关闭全部后，抽屉首页呈现英文网址输入、Tools 与 Recommended，旧标签和关闭确认均消失。"))
            guard try String(contentsOf: fixture.start, encoding: .utf8) == "EXTERNAL DISK VERSION E2E\n" else {
                throw E2EError.product("放弃草稿后磁盘上的外部版本改变")
            }
            return "取消与冲突均保留两个原始标签和草稿，明确放弃后才关闭全部，外部版本保留"
        }
    }

    private static func openFilesTool(_ session: Session) async throws {
        try await session.visualClick("点击右抽屉首页 Tools 分组中的 Files 磁贴，避开 Recommended 分组")
    }

    private static func menuItem(_ session: Session, _ title: String) async throws {
        try await session.visualClick("点击当前右侧标签操作菜单中对应 \(title) 的一行，不点击其他同前缀项目或工具提示")
    }
}
