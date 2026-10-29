import E2EKit
import Fixtures
import Foundation

/// Combo首页：两主题×两壁纸，12/1空间，搜索、键盘、菜单与状态。
@MainActor
public enum WorkspaceHomeDesign {
    public nonisolated static let name = "workspace-home-design"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("建立隔离数据库") {
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            session.stop()
            return "使用隔离数据，保留用户App"
        }
        for (theme, themeName) in [("sapphire-night", "dark"), ("graphite", "graphite")] {
            for alternate in [false, true] {
                let prefix = "\(themeName)-\(alternate ? "blue" : "default")"
                try await session.step("\(prefix)：12个空间默认首页") {
                    try HomeDesignFixture.install(database: session.databaseURL(), count: 12, theme: theme, alternate: alternate)
                    try session.launch(language: "zh-Hans")
                    try await session.resize(width: 1440, height: 840)
                    let frame = try await session.wait(Expectation(allOf: ["Norma OS", "最近访问"], absent: ["继续工作"], timeout: 25))
                    try session.save(frame, "\(prefix)-01-default.png")
                    let catalog = try session.catalog()
                    guard catalog.names.count == 12 else { throw E2EError.product("应有12个空间：\(catalog.summary)") }
                    return "最近访问只露四张；布局和壁纸由看图清单核验"
                }
                try await session.step("\(prefix)：悬停与键盘焦点") {
                    let frame = try await session.capture()
                    try await session.hover(local: CGPoint(x: 300, y: 505), on: frame)
                    try session.save(try await session.capture(), "\(prefix)-02-hover.png")
                    try session.save(try await session.capturePressed(local: CGPoint(x: 300, y: 505), on: frame), "\(prefix)-02-pressed.png")
                    try await session.shortcut(key: 48, modifiers: [])
                    try session.save(try await session.capture(), "\(prefix)-03-focus.png")
                    return "真实hover/Tab，检查2px上浮与3px焦点环"
                }
                try await session.step("\(prefix)：卡片菜单与取消删除") {
                    try await openFirstMenu(session)
                    let menu = try await session.wait(Expectation(allOf: ["重命名", "删除"], timeout: 5))
                    try session.save(menu, "\(prefix)-04-menu.png")
                    try await session.click(matching: ["删除"], pick: .bottom)
                    let confirmation = try await session.wait(Expectation(allOf: ["删除工作空间"], timeout: 5))
                    try session.save(confirmation, "\(prefix)-05-delete-confirm.png")
                    try await session.click(matching: ["取消"], pick: .bottom)
                    _ = try await session.wait(Expectation(allOf: ["最近访问"], absent: ["删除工作空间"], timeout: 5))
                    guard try session.catalog().names.count == 12 else { throw E2EError.product("取消删除不能改变目录") }
                    return "菜单点击未打开空间，删除经过既有确认并可取消"
                }
                try await session.step("\(prefix)：原位搜索、分组与折叠") {
                    try await session.click(matching: ["全部 12", "全部12"], pick: .top)
                    let search = try await session.wait(Expectation(anyOf: ["最近 7 天", "最近7天"], allOf: ["更早"], timeout: 5))
                    try session.save(search, "\(prefix)-06-search.png")
                    try await session.click(matching: ["显示全部"], pick: .bottom)
                    _ = try await session.wait(Expectation(allOf: ["收起"], timeout: 5))
                    try await session.scroll(local: CGPoint(x: 720, y: 620), pixels: -650, on: try await session.capture())
                    let expanded = try await session.wait(Expectation(allOf: ["性能调优", "发布流水线", "插件市场"], timeout: 5))
                    try session.save(expanded, "\(prefix)-07-older-expanded.png")
                    try await session.shortcut(key: 53, modifiers: [])
                    _ = try await session.wait(Expectation(allOf: ["最近访问"], absent: ["更早"], timeout: 5))
                    return "全部入口展开、折叠项可达、Esc关闭"
                }
                if !alternate { try await verifyHomeFocus(session, prefix: prefix) }
                try await session.step("\(prefix)：实时筛选与回车打开") {
                    try await session.click(matching: ["搜索"], pick: .top)
                    try await session.type("插件")
                    _ = try await session.wait(Expectation(allOf: ["插件市场", "搜索结果"], timeout: 5))
                    // 查询会改变面板高度，等260ms展开动画与合成帧后检查完整底部提示。
                    try await Task.sleep(nanoseconds: 350_000_000)
                    try session.save(try await session.capture(), "\(prefix)-08-match.png")
                    try await session.shortcut(key: 125, modifiers: [])
                    try await session.shortcut(key: 36, modifiers: [])
                    _ = try await session.wait(Expectation(allOf: ["插件市场"], absent: ["最近访问", "搜索结果"], timeout: 12))
                    try await session.shortcut(key: 40, modifiers: [0x37, 0x38])
                    // 最近卡片会先于首页搜索入口出现在返回转场中；入口可见后才继续下一步。
                    _ = try await session.wait(Expectation(allOf: ["最近访问", "插件市场", "搜索"], timeout: 8))
                    return "回车打开原本不在最近四个中的空间，返回后按最近访问置顶"
                }
                try await session.step("\(prefix)：无结果预填创建") {
                    try await session.click(matching: ["搜索"], pick: .top)
                    try await session.type("HomeNewDraft")
                    let empty = try await session.wait(Expectation(allOf: ["没有名为", "HomeNewDraft"], timeout: 5))
                    try session.save(empty, "\(prefix)-09-empty.png")
                    try await session.click(matching: ["新建“HomeNewDraft”", "新建"], pick: .bottom)
                    let dialog = try await session.wait(Expectation(allOf: ["工作空间名称", "HomeNewDraft"], timeout: 5))
                    try session.save(dialog, "\(prefix)-10-prefilled-create.png")
                    try await session.click(matching: ["取消"], pick: .bottom)
                    session.stop()
                    return "无结果可预填创建，取消不写目录"
                }
                try await session.step("\(prefix)：单工作空间与新建卡") {
                    try HomeDesignFixture.install(database: session.databaseURL(), count: 1, theme: theme, alternate: alternate)
                    try session.launch(language: "zh-Hans")
                    try await session.resize(width: 1440, height: 840)
                    let few = try await session.wait(Expectation(allOf: ["Norma OS", "最近访问"], absent: ["全部 12"], timeout: 25))
                    try session.save(few, "\(prefix)-11-one.png")
                    session.stop()
                    return "一张空间卡与一张新建虚线卡整体居中"
                }
            }
        }
        try await runHomeLifecycle(session)
    }

    private static func openFirstMenu(_ session: Session) async throws {
        let frame = try await session.capture()
        // 1440×840响应布局：首卡x192/y436，卡宽252，菜单触发器右距6、宽36，中心x420/y607。
        try await session.hover(local: CGPoint(x: 420, y: 607), on: frame)
        let fresh = try await session.capture()
        try await session.click(local: CGPoint(x: 420, y: 607), on: fresh)
    }
}
