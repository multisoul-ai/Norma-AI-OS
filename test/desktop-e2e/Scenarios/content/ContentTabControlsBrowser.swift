import CoreGraphics
import E2EKit
import Fixtures
import Foundation

@MainActor
extension ContentTabControls {
    static func refreshBrowser(_ session: Session, fixture: ContentTabControlsFixture, server: ContentBrowserFixture,
                               resources: [String], english: Bool) async throws {
        try await session.step("按域名定位回环网页，验证浮层层级与页面会话") {
            if !english {
                let frame = try await session.capturePixels()
                let width = frame.windowFrame.width
                try await session.drag(local: CGPoint(x: width - compactWidth, y: 260), to: CGPoint(x: width * 0.63, y: 260), on: frame)
            }
            try await search(session)
            if english {
                try await check(session, "01-search-en.png", VisualCriterion(id: "search", text: "蓝宝石之夜主题。右侧搜索浮层聚焦，英文占位提示可按名称、路径或域名搜索，当前 First.txt 带勾标，三条候选和图标排版完整。"))
            }
            try await session.type("127.0.0.1")
            if !english {
                try await check(session, "10-domain-search.png", VisualCriterion(id: "domain", text: "以域名 127.0.0.1 过滤，只得到 Loopback 网页行，其网页图标和回环 URL 可见，其他 localhost 网页和文件不匹配。"))
            }
            _ = try await waitTabs(session, order: resources, selected: english ? fixture.start.path : fixture.zulu.path)
            try await session.shortcut(key: 125, modifiers: [])
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await waitTabs(session, order: resources, selected: server.refreshAddress)
            try await check(session, english ? "02-web-before-en.png" : "11-web-before.png", VisualCriterion(id: "web", text: "已选 Loopback 网页，真实 WKWebView 正文标题 WEB BEFORE、NEXT PAGE E2E 链接与 Session marker 文本框可见。搜索浮层收起。"))
            let requests = fixture.requestCount
            guard requests > 0 else { throw E2EError.product("读不到实际回环 URL 请求") }
            if !english {
                try await typeInWebView(session, text: "SESSION KEPT")
                try await search(session)
                try await check(session, "12-search-over-web.png", VisualCriterion(id: "layer", text: "网页为当前标签时，搜索浮层和输入框在网页上方，未被 WKWebView 遮挡。Loopback 带当前项勾标，标签滚动区与固定控件不互相盖住。"))
                try await session.visualClick("点击搜索浮层外、抽屉左侧画布的空白位置，以关闭浮层。不要点项目卡片或 Dock。")
                try await actions(session)
                try await check(session, "13-menu-over-web.png", VisualCriterion(id: "layer", text: "Loopback 的标签操作菜单完整可见，原生网页层没有盖住菜单，末位的「关闭右侧标签页」禁用，菜单没有把搜索留在画面。"))
                try await session.shortcut(key: 53, modifiers: [])
                try await typeInWebView(session, text: " INTACT", replacing: false)
                try await check(session, "14-web-session-restored.png", VisualCriterion(id: "session", text: "菜单和搜索浮层均收起，原页面仍显示 WEB BEFORE。网页里的真实输入框显示 SESSION KEPT INTACT，证明内容和网页输入在浮层关闭后仍可用。"))
                guard fixture.requestCount == requests else { throw E2EError.product("开关搜索或菜单时重新请求了网页") }
                _ = try await waitTabs(session, order: resources, selected: server.refreshAddress)
            }
            return "由真实 HTTP 响应渲染回环 URL，底层标签资源未被浮层改变"
        }

        try await session.step("保持实际 HTTP 响应，验收刷新中及同地址新正文") {
            let ids = try await waitTabs(session, order: resources, selected: server.refreshAddress).map(\.id)
            let refresh = try await session.visualLocate("定位右抽屉标签条右端的刷新圆箭头中心（刷新当前标签 / Refresh current tab）")
            // Locate fixed controls and the two fully visible neighboring tabs before holding the
            // server. The model only reviews the snapshots after the HTTP gate is released.
            let file = english ? nil : try await session.visualLocate("定位标签条内、紧邻当前 Loopback 左侧的 Plan.txt 标签胶囊中间，避开关闭图标")
            let web = english ? nil : try await session.visualLocate("定位标签条内 Loopback 标签胶囊中间，避开关闭图标")
            let reload: (frame: Frame, point: CGPoint)
            if english {
                try await actions(session)
                reload = try await session.visualLocate("定位标签操作菜单分隔线下面的 Reload 项中心")
            } else {
                reload = refresh
            }
            let before = fixture.requestCount
            let hold = fixture.webState.appendingPathComponent("hold")
            try fixture.write("WEB UPDATED", to: fixture.webState.appendingPathComponent("body.txt"))
            try Data().write(to: hold)
            defer { try? FileManager.default.removeItem(at: hold) }
            try await session.click(local: reload.point, on: reload.frame)
            let deadline = Date().addingTimeInterval(4)
            while fixture.requestCount == before && Date() < deadline {
                try await Task.sleep(nanoseconds: 80_000_000)
            }
            guard fixture.requestCount == before + 1 else { throw E2EError.product("刷新未发出唯一的新 HTTP 请求") }
            let busy = try await session.capturePixels()
            try await session.click(local: refresh.point, on: busy) // A second request must not cancel or duplicate reload.
            var switched: Frame?
            var returned: Frame?
            if let file, let web {
                try await session.click(local: file.point, on: try await session.capturePixels())
                _ = try await waitTabs(session, order: resources, selected: fixture.zulu.path)
                try await Task.sleep(nanoseconds: 250_000_000)
                switched = try await session.capturePixels()
                try await session.click(local: web.point, on: try await session.capturePixels())
                _ = try await waitTabs(session, order: resources, selected: server.refreshAddress)
                try await Task.sleep(nanoseconds: 250_000_000)
                returned = try await session.capturePixels()
            }
            try FileManager.default.removeItem(at: hold)
            let busyLabel = english ? "「Refreshing…」" : "「正在刷新…」"
            try await snapshot(session, busy, english ? "03-web-refreshing-en.png" : "15-web-refreshing.png",
                VisualCriterion(id: "refreshing", text: "Loopback 仍是当前标签，出现转圈、细进度条和\(busyLabel)，刷新入口显示忙碌/禁用反馈，WEB UPDATED 尚未出现。"))
            if let switched, let returned {
                try await snapshot(session, switched, "16-refresh-other-tab.png", VisualCriterion(id: "file", text: "刷新响应尚未放行时已切到 Plan.txt，显示实际文件正文 ZULU RECOVERED。当前文件没有继承网页的刷新中提示或禁用刷新状态。"))
                try await snapshot(session, returned, "17-refresh-returned.png", VisualCriterion(id: "web", text: "同一请求响应尚未放行时已切回 Loopback，刷新中指示回到当前内容，尚未显示 WEB UPDATED。"))
            }
            try await check(session, english ? "04-web-updated-en.png" : "18-web-updated.png", VisualCriterion(id: "updated", text: "Loopback 仍选中，同一地址的新网页正文显示 WEB UPDATED，没有 WEB BEFORE、刷新失败或持续刷新的指示。"))
            let after = try await waitTabs(session, order: resources, selected: server.refreshAddress)
            guard after.map(\.id) == ids else { throw E2EError.product("网页刷新替换了原标签身份") }
            guard fixture.requestCount == before + 1 else { throw E2EError.product("刷新期间重复点击或切换标签产生额外 HTTP 请求") }
            return "同一回环地址新收到且只收到一个请求，新正文实际可见"
        }
    }

    static func actions(_ session: Session) async throws {
        try await session.visualClick("点击右侧内容上下文栏常驻的 ⋯ 标签操作（Tab actions）按钮，位于标签条下一行或工作台头部")
    }

    private static func typeInWebView(_ session: Session, text: String, replacing: Bool = true) async throws {
        let input = try await session.visualLocate("定位 Loopback 网页正文中的 Session marker 输入框内部，已有 SESSION KEPT 时仍点同一文本框，不点浏览器地址栏")
        session.appKitInputOverride = true
        defer { session.appKitInputOverride = false }
        try await session.click(local: input.point, on: input.frame)
        if replacing { try await session.selectAll() }
        else { try await session.shortcut(key: 124, modifiers: [0x37]) } // Cmd+Right: end of the input.
        try await session.type(text)
    }

    private static func snapshot(_ session: Session, _ frame: Frame, _ name: String, _ criterion: VisualCriterion) async throws {
        try session.save(try await session.visualSnapshotCheck(frame, [criterion]), name)
    }
}
