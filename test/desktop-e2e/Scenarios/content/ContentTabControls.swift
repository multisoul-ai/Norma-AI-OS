import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// Real drawer input on a 24-tab workspace. SQLite proves tab identity and order; the window proves rendered content.
@MainActor
public enum ContentTabControls {
    public nonisolated static let name = "content-tab-controls"
    public nonisolated static let closeName = "content-tab-controls-close"
    static let compactWidth: CGFloat = 250

    public static func run(_ session: Session, closing: Bool = false) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        let language = closing ? "en" : "zh-Hans"
        try session.launch(language: language)
        defer { session.stop() }
        try await session.waitContentSchema(timeout: 20)
        session.stop()
        let database = try session.databaseURL()
        guard let root = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
        let fixture = ContentTabControlsFixture(dataRoot: root)
        defer { session.stop(); try? FileManager.default.removeItem(at: fixture.root) }
        let server = try ContentBrowserFixture(refreshDirectory: fixture.webState)
        defer { server.stop() }
        let resources = try fixture.install(database: database, server: server, many: !closing)
        try await session.step("打开隔离工作区及右侧抽屉") {
            try await openDrawer(session, language: language)
            let tabs = try await waitTabs(session, order: resources, selected: fixture.start.path)
            let webCount = tabs.filter { $0.kind == "browser" }.count
            let fileCount = tabs.filter { $0.kind == "file" }.count
            // 20 alternating fillers, three named files and one target website in the large fixture.
            guard closing ? (webCount == 1 && fileCount == 3) : (webCount == 11 && fileCount == 13) else {
                throw E2EError.product("混合标签夹具的类型或数量错误：网页 \(webCount)，文件 \(fileCount)")
            }
            return "已恢复 \(webCount) 个网页及 \(fileCount) 个文件标签，First.txt 选中"
        }
        if closing {
            try await closeTabs(session, fixture: fixture, server: server, resources: resources)
        } else {
            try await searchTabs(session, fixture: fixture, resources: resources)
            try await refreshFile(session, fixture: fixture, resources: resources)
            try await refreshBrowser(session, fixture: fixture, server: server, resources: resources, english: false)
        }
    }

    static func openDrawer(_ session: Session, language: String) async throws {
        try session.launch(language: language)
        try await session.resize(width: 1440, height: 800)
        try await session.visualClick("从工作空间目录打开 \(ContentTabControlsFixture.workspace) 工作空间，点击卡片标题或其打开按钮")
        try await session.visualClick("点击画布上 \(ContentTabControlsFixture.project) 项目卡片的标题进入项目，不点右上角内容抽屉")
        // The collapsed reader toggle is a 32pt button 12pt from the right edge at y=10.
        let frame = try await session.capturePixels()
        try await session.click(local: CGPoint(x: frame.windowFrame.width - 28, y: 26), on: frame)
        // The initial input is often a drag on the separator, which moves during the 220ms reveal.
        try await Task.sleep(nanoseconds: 350_000_000)
    }

    @discardableResult
    static func waitTabs(_ session: Session, order: [String], selected: String?) async throws -> [ContentTabRow] {
        let deadline = Date().addingTimeInterval(5)
        let expectedSelection = selected.map { [$0] } ?? []
        var rows: [ContentTabRow] = []
        repeat {
            rows = try session.contentTabs(workspaceName: ContentTabControlsFixture.workspace)
            if rows.map(\.resource) == order && rows.filter(\.selected).map(\.resource) == expectedSelection {
                return rows
            }
            try await Task.sleep(nanoseconds: 120_000_000)
        } while Date() < deadline
        throw E2EError.product("标签顺序或选中不符，预期 \(order.count) 项且选中 \(selected ?? "无")，实际 \(rows.map { "\($0.resource)\($0.selected ? "*" : "")" })")
    }

    static func check(_ session: Session, _ name: String, _ criteria: VisualCriterion...) async throws {
        try session.save(try await session.visualCheck(criteria), name)
    }

    static func search(_ session: Session) async throws {
        try await session.visualClick("点击右抽屉标签条右端固定的放大镜「搜索标签」按钮，区别于新建加号和刷新图标")
    }

    private static func searchTabs(_ session: Session, fixture: ContentTabControlsFixture, resources: [String]) async throws {
        try await session.step("窄栏中搜索同名标签，区分路径并用 Esc 退出") {
            let frame = try await session.capturePixels()
            let width = frame.windowFrame.width
            // The default width is 37% of the window. 250pt exercises the compact tools below 280pt.
            try await session.drag(local: CGPoint(x: width * 0.63, y: 260), to: CGPoint(x: width - compactWidth, y: 260), on: frame)
            let narrow = try await session.visualCheck([VisualCriterion(id: "narrow", text: "右抽屉约 250pt 宽，First.txt 选中且正文为 FILE BEFORE。标签条产生横向溢出，加号与另起一行的刷新、搜索，以及下方上下文栏的 ⋯ 仍完整、分离、可见。")])
            try session.save(narrow, "01-narrow-default.png")
            // The strip translates a vertical wheel tick into horizontal movement while hovered.
            let strip = CGPoint(x: width - compactWidth + 100, y: 26)
            try await session.scroll(local: strip, pixels: -280, on: narrow)
            let scrolled = try await session.visualCheck([VisualCriterion(id: "scroll", text: "标签胶囊横向滚动，标签条内的 First.txt 已离开可见区域并显示更后的标签，常驻控件位置稳定。上下文和正文仍为 First.txt 与 FILE BEFORE。")])
            try session.save(scrolled, "01b-narrow-scrolled.png")
            try await session.scroll(local: strip, pixels: 280, on: scrolled)
            try await search(session)
            // Typing immediately exercises the focus granted when the popup opens.
            try await session.type("Plan.txt")
            try await check(session, "02-duplicate-sources.png", VisualCriterion(id: "duplicates", text: "聚焦的搜索框实际输入了 Plan.txt。恰好两条同名 Plan.txt 文件行，其文件图标和 alpha-review、zulu-review 各自的长来源可区别，字不盖住相邻行。右栏未切换到 ALPHA FILE 或 ZULU FILE。"))
            let source = try await session.visualLocate("定位搜索结果里来源含 zulu-review 的第二条 Plan.txt 行中央，用于悬停显示完整来源")
            try await session.hover(local: source.point, on: source.frame)
            try await check(session, "02b-full-source.png", VisualCriterion(id: "source", text: "悬停提示完整显示路径 \(fixture.zulu.path)，从 /private/tmp 到 Plan.txt 均可读，无窗外裁切。仅悬停时底层没有切换到 ZULU FILE。"))
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            try await session.shortcut(key: 53, modifiers: []) // Esc
            // Enter from restored trigger focus should reopen it. Type a query with zero matches without clicking the input.
            try await session.shortcut(key: 36, modifiers: [])
            try await session.type("missing-zz-tab")
            try await check(session, "03-search-empty.png", VisualCriterion(id: "empty", text: "搜索已重新打开且查询为 missing-zz-tab，显示「未找到匹配的标签」，没有标签结果行，First.txt 保持当前。"))
            try await session.shortcut(key: 48, modifiers: []) // Tab
            try await check(session, "04-search-dismissed.png", VisualCriterion(id: "dismiss", text: "Tab 已退出搜索浮层。First.txt 和 FILE BEFORE 仍可见，画面没有搜索框或未匹配提示，搜索或后继控件具有清楚的键盘焦点反馈。"))
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            return "标题搜索得到两个来源，零结果及两种键盘退出均保留原选中项"
        }
        try await session.step("按远端路径过滤，并用方向键和 Enter 定位第 23 个标签") {
            try await search(session)
            try await session.type("zulu-review")
            try await session.shortcut(key: 125, modifiers: []) // Down
            try await session.shortcut(key: 126, modifiers: []) // Up, one result remains highlighted.
            try await check(session, "05-path-keyboard.png", VisualCriterion(id: "path", text: "查询 zulu-review，唯一结果是含 zulu-review 路径的 Plan.txt，键盘高亮与选中勾标可区分，alpha-review 不在列表。底层仍选择 First.txt。"))
            _ = try await waitTabs(session, order: resources, selected: fixture.start.path)
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await waitTabs(session, order: resources, selected: fixture.zulu.path)
            try await check(session, "06-path-revealed.png", VisualCriterion(id: "revealed", text: "搜索收起，选中的 Plan.txt 完整滚入窄栏标签条，未被固定按钮或滚动边缘裁切。正文为 ZULU FILE，未显示 ALPHA FILE 或 FILE BEFORE。"))
            return "第 23 项按稳定文件资源选中，正文和完整可见标签与数据库一致"
        }
    }

    private static func refreshFile(_ session: Session, fixture: ContentTabControlsFixture, resources: [String]) async throws {
        try await session.step("重读真实文件，失败保留旧正文并可重试") {
            let ids = try await waitTabs(session, order: resources, selected: fixture.zulu.path).map(\.id)
            try fixture.write("ZULU UPDATED", to: fixture.zulu)
            try await session.visualClick("点击右抽屉标签条右端的圆箭头「刷新当前标签」，不要点放大镜或加号")
            try await check(session, "07-file-updated.png", VisualCriterion(id: "file", text: "Plan.txt 同一标签正文已经变为 ZULU UPDATED，旧 ZULU FILE 不再显示，无刷新失败提示或持续进度条。"))
            try FileManager.default.removeItem(at: fixture.zulu)
            try await session.visualClick("再次点击标签条右端圆箭头刷新当前 Plan.txt")
            try await check(session, "08-file-refresh-failed.png", VisualCriterion(id: "failure", text: "文件刷新失败显示精简中文「刷新失败」和「重试」入口，旧的可用正文 ZULU UPDATED 仍可阅读，错误提示没有盖住或替换正文。"))
            try fixture.write("ZULU RECOVERED", to: fixture.zulu)
            try await session.visualClick("点击右抽屉文件刷新错误提示内的「重试」")
            try await check(session, "09-file-recovered.png", VisualCriterion(id: "retry", text: "Plan.txt 正文显示 ZULU RECOVERED，旧 ZULU UPDATED 和刷新失败、重试提示均消失。"))
            let after = try await waitTabs(session, order: resources, selected: fixture.zulu.path)
            guard after.map(\.id) == ids else { throw E2EError.product("刷新和重试替换了原文件标签身份") }
            return "实际夹具改写、移除及恢复分别驱动刷新成功、保留旧预览、重试成功"
        }
    }
}
