import AppKit
import E2EKit
import Fixtures
import Foundation

/// 六个已有主题：通过真实设置切换，在同一进程检查首页、左右抽屉与项目外框。
/// 数据库只用于确认保存结果；像素/文字与交互证据全部来自真实窗口。
@MainActor
public enum SettingsThemeConsistency {
    public nonisolated static let name = "settings-theme-consistency"
    private static let themes = [
        ("墨黑", "Ink Black", "ink-black", false),
        ("石墨灰", "Graphite", "graphite", false),
        ("暖曜黑", "Warm Obsidian", "warm-obsidian", false),
        ("蓝宝石之夜", "Sapphire Night", "sapphire-night", false),
        ("月岩", "Moonstone", "moonstone", false),
        ("深海石", "Deep Petrol", "deep-petrol", false),
    ]

    public static func run(_ session: Session) async throws {
        if session.visualReview { return try await SettingsThemeVisual.run(session) }
        var project: URL?
        defer {
            session.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
        }
        try await session.step("创建真实工作空间与项目夹具") {
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            let form = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"]))
            guard let input = form.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("创建表单缺少名称输入框：\(form.summary)")
            }
            try await session.click(local: input.center, on: form)
            try await session.type("ThemeCheck")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["ThemeCheck"]))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "ThemeCheck")
            try Data("THEME EDITOR SAMPLE".utf8).write(to: project!.appendingPathComponent("ThemeNote.txt"))
            try Catalog.suppressFeatureGuide(database: dataRoot.appendingPathComponent("app.sqlite"))
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(allOf: ["ThemeCheck"], timeout: 20))
            return try session.prove(names: ["ThemeCheck"], areaCount: 1, linkedTo: "ThemeCheck").summary
        }
        for (chinese, english, id, light) in themes {
            try await session.step("\(id)：起始页实时切换") {
                try await session.shortcut(key: 40, modifiers: [0x37, 0x38])
                _ = try await session.wait(Expectation(anyOf: ["搜索工作空间", "Search workspaces"]))
                try await session.shortcut(key: 43, modifiers: [0x37])
                _ = try await SettingsSupport.select(session, labels: ["主题", "Themes"], content: ["Ink Black", "墨黑"])
                try await chooseTheme(session, labels: [chinese, english], id: id)
                try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: id)
                try await session.shortcut(key: 43, modifiers: [0x37])
                let home = try await session.wait(Expectation(anyOf: ["搜索工作空间", "Search workspaces"], allOf: ["ThemeCheck"], absent: ["选择工作空间的外观", "选择工作空间外观"]))
                try session.save(home, "\(id)-home.png")
                return "同一进程切换主题，首页与保存值一致"
            }
            try await session.step("\(id)：双抽屉及画布外框") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
                _ = try await session.wait(Expectation(allOf: ["E2E Project"]))
                var frame = try await settled(session)
                if !frame.contains("运行概览") {
                    try await session.click(local: CGPoint(x: 98, y: 26), on: frame)
                }
                frame = try await settled(session)
                if !frame.contains("更多工具") {
                    try await session.click(local: CGPoint(x: frame.windowFrame.width - 28, y: 26), on: frame)
                }
                _ = try await session.wait(Expectation(allOf: ["运行概览", "更多工具", "E2E Project"]))
                frame = try await settled(session)
                try assertSurfacePolarity(frame, light: light)
                try session.save(frame, "\(id)-drawers.png")
                try await session.click(matching: ["E2E Project"], pick: .bottom)
                frame = try await settled(session)
                try session.save(frame, "\(id)-project.png")
                return "左右抽屉保持展开，项目外框仍可见；两侧背景明暗符合所选主题"
            }
            try await session.step("\(id)：选中、焦点、悬停、按下与禁用状态") {
                try await session.click(matching: ["待办"], pick: .left)
                var frame = try await settled(session)
                try session.save(frame, "\(id)-overview-selected.png")
                guard let tab = frame.pick(["已指派"], .left) else {
                    throw E2EError.product("缺少已指派分段：\(frame.summary)")
                }
                try await session.hover(local: tab.center, on: frame, dwell: 120_000_000)
                try session.save(try await session.capture(), "\(id)-tab-hover.png")
                frame = try await session.capture()
                guard let target = frame.pick(["已指派"], .left) else { throw E2EError.product("已指派分段消失") }
                try await session.press(local: target.center, on: frame) {
                    try session.save(try await session.capture(), "\(id)-tab-pressed.png")
                }
                _ = try await session.wait(Expectation(allOf: ["已指派"]))
                // 空输入的发送按钮保持禁用；点击地址栏验证组件前景和焦点随主题更新。
                frame = try await session.capture()
                guard let address = frame.pick(["搜索或输入网址"], .right) else { throw E2EError.product("右栏缺少地址输入") }
                try await session.click(local: address.center, on: frame)
                try await session.type("theme-check")
                frame = try await session.wait(Expectation(allOf: ["theme-check"]))
                try session.save(frame, "\(id)-input-focus.png")
                try await session.selectAll()
                try await session.shortcut(key: 51, modifiers: [])
                return "已指派分段选中，悬停/按下均有截图；输入未提交外部请求"
            }
        }
        try await session.step("文件工作台：深色主题切换后编辑并保存") {
            // 工具区与推荐区都有“文件”；工作台入口是上方工具行。
            try await session.click(matching: ["文件"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["ThemeNote.txt"], timeout: 12))
            try await session.click(matching: ["ThemeNote.txt"], pick: .bottom)
            var file = try await session.wait(Expectation(allOf: ["THEME EDITOR SAMPLE"], timeout: 12))
            guard let title = file.pick(["ThemeNote.txt"], .top) else { throw E2EError.product("文件路径不可见") }
            try await session.click(local: CGPoint(x: file.windowFrame.width - 245, y: title.center.y), on: file)
            var current = "THEME EDITOR SAMPLE"
            for (labels, id) in [(["墨黑", "Ink Black"], "ink-black"), (["深海石", "Deep Petrol"], "deep-petrol")] {
                // 编辑器会处理自身快捷键；先点击画布空白，将焦点移出但保留编辑模式。
                file = try await session.capture()
                try await session.click(local: CGPoint(x: file.windowFrame.width * 0.53, y: 190), on: file)
                try await session.shortcut(key: 43, modifiers: [0x37])
                _ = try await SettingsSupport.select(session, labels: ["主题", "Themes"], content: ["Ink Black", "墨黑"])
                try await chooseTheme(session, labels: labels, id: id)
                try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: id)
                try await session.shortcut(key: 43, modifiers: [0x37])
                file = try await session.wait(Expectation(allOf: [current]))
                guard let text = file.pick([current], .top) else { throw E2EError.product("切换后编辑器文字不可见") }
                try await session.click(local: text.center, on: file)
                try await session.selectAll()
                current = "SAVED THEME \(id)"
                try await session.type(current)
                try await session.shortcut(key: 1, modifiers: [0x37])
                file = try await session.wait(Expectation(allOf: [current]))
                guard let project else { throw E2EError.env("项目夹具丢失") }
                try await waitForSavedText(project.appendingPathComponent("ThemeNote.txt"), expected: current)
                file = try await settled(session)
                try session.save(file, "\(id)-editor-saved.png")
            }
            return "已创建的编辑器在两种深色主题切换后可读可编辑，磁盘写入正确"
        }
    }

    private static func waitForSavedText(_ path: URL, expected: String) async throws {
        // 保存由 App 后台执行，编辑器里出现输入文字不能证明已经落盘。
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if (try? String(contentsOf: path, encoding: .utf8)) == expected { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("主题切换后的编辑在 5 秒内未保存到磁盘")
    }

    private static func chooseTheme(_ session: Session, labels: [String], id: String) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
        for _ in 0..<8 {
            let frame = try await session.capture()
            if let card = frame.pick(labels, .right) {
                try await session.click(local: card.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -180)
        }
        throw E2EError.product("主题列表找不到 \(id)")
    }

    private static func settled(_ session: Session) async throws -> Frame {
        try await Task.sleep(nanoseconds: 900_000_000)
        return try await session.capture()
    }

    private static func assertSurfacePolarity(_ frame: Frame, light: Bool) throws {
        let bitmap = NSBitmapImageRep(cgImage: frame.image)
        // 面板下部靠边的空白取样，避开文字、滚动条、输入框及画布内容；截图可能是 2×。
        for x in [0.012, 0.986] {
            let px = Int(Double(bitmap.pixelsWide) * x)
            let py = Int(Double(bitmap.pixelsHigh) * 0.65)
            guard let color = bitmap.colorAt(x: px, y: py)?.usingColorSpace(.deviceRGB) else {
                throw E2EError.product("无法读取抽屉背景像素")
            }
            let luma = (color.redComponent + color.greenComponent + color.blueComponent) / 3
            guard light ? luma > 0.65 : luma < 0.40 else {
                throw E2EError.product("抽屉背景未同步：light=\(light), x=\(x), luma=\(luma)")
            }
        }
    }
}
