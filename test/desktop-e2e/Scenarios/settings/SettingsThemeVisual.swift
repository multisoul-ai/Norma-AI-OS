import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 使用真实控制通道与模型看图，不调用系统 Vision OCR。
@MainActor
enum SettingsThemeVisual {
    static func run(_ session: Session) async throws {
        let available = [("墨黑", "ink-black"), ("石墨灰", "graphite"), ("暖曜黑", "warm-obsidian"), ("蓝宝石之夜", "sapphire-night"), ("月岩", "moonstone"), ("深海石", "deep-petrol")]
        let requested = ProcessInfo.processInfo.environment["NORMA_THEME_REVIEW_THEME"]
        let themes = available.filter { requested == nil || requested == "mineral" && ["moonstone", "deep-petrol"].contains($0.1) || requested == $0.1 }
        guard let first = themes.first else { throw E2EError.env("未知深色主题：\(requested ?? "")") }
        let terminalStyle = "深底柔白终端，标题与外框使用同色系的深色材质"
        let fakeShell = try FakeAgentShell.install()
        let previousZdot = ProcessInfo.processInfo.environment["ZDOTDIR"]
        let previousApproval = ProcessInfo.processInfo.environment["NORMA_FAKE_APPROVAL"]
        setenv("ZDOTDIR", fakeShell.path, 1)
        setenv("NORMA_FAKE_APPROVAL", "1", 1)
        var project: URL?
        defer {
            session.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
            try? FileManager.default.removeItem(at: fakeShell)
            if let previousZdot { setenv("ZDOTDIR", previousZdot, 1) } else { unsetenv("ZDOTDIR") }
            if let previousApproval { setenv("NORMA_FAKE_APPROVAL", previousApproval, 1) } else { unsetenv("NORMA_FAKE_APPROVAL") }
        }
        try await session.step("建立深色主题真实窗口夹具") {
            try session.launch(language: "zh-Hans")
            let db = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            try Catalog.seedWorkspace(database: db, name: "DarkThemeCheck")
            try Catalog.suppressFeatureGuide(database: db)
            try Catalog.seedSetting(database: db, key: "appearance.theme.v1", value: first.1)
            project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: "DarkThemeCheck")
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1440, height: 840)
            return try session.prove(names: ["DarkThemeCheck"], areaCount: 1, linkedTo: "DarkThemeCheck").summary
        }
        for (index, theme) in themes.enumerated() {
            let (name, id) = theme
            try await session.step("\(id)：设置持久化与起始页") {
                if index > 0 {
                    try await session.shortcut(key: 40, modifiers: [0x37, 0x38])
                    try await session.shortcut(key: 43, modifiers: [0x37])
                    try await session.scroll(xFraction: 0.25, yFraction: 0.5, deltaY: 1200)
                    try await session.visualClick("设置侧栏中定位「主题」入口；只选择外观主题分类")
                    try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
                    try await session.visualClick("主题网格中定位「\(name)」卡片中心，切换到该现有主题")
                    try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: id)
                    try await session.shortcut(key: 43, modifiers: [0x37])
                }
                let home = try await session.visualCheck([
                    VisualCriterion(id: "home", text: "当前是 DarkThemeCheck 工作空间起始页；深色背景、柔白标题、搜索与卡片文字清晰，布局完整，不残留浅色材质"),
                    VisualCriterion(id: "accent", text: "\(name) 的强调色用于按钮和焦点，内容层级清楚：墨黑为银灰，石墨灰为雾蓝，暖曜黑为香槟金；蓝宝石为冷蓝，月岩为银灰微紫，深海石为低饱和灰青")
                ])
                try session.save(home, "\(id)-home.png")
                try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: id)
                return "真实首页已检查，持久化值匹配 \(id)"
            }
            try await session.step("\(id)：设置目录与选中标记") {
                try await session.shortcut(key: 43, modifiers: [0x37])
                try await session.visualClick("定位设置侧栏的「主题」入口，进入六主题选择器")
                try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
                let settings = try await session.visualCheck([
                    VisualCriterion(id: "catalog", text: "目录包含墨黑、石墨灰、暖曜黑、蓝宝石之夜、月岩和深海石，不再有午夜紫或森林薄荷"),
                    VisualCriterion(id: "selection", text: "\(name) 的选中勾是清晰深色，在浅强调色圆形底上可辨；其他卡片未选中，不误用固定白勾")
                ])
                try session.save(settings, "\(id)-settings.png")
                try await session.shortcut(key: 43, modifiers: [0x37])
                return "目录和选中勾已看图"
            }
            try await session.step("\(id)：项目外框、双抽屉与终端") {
                try await click(session, x: 1329, y: 559)
                let projectFrame = try await session.visualCheck([
                    VisualCriterion(id: "project", text: "E2E Project 项目卡片可见，项目标题和边界在深色环境里清楚，与所选深色主题一致，没有残留浅色外框")
                ])
                try session.save(projectFrame, "\(id)-project.png")
                // 抽屉展开后项目卡片会重排，必须按当前画面定位，不能复用首轮坐标。
                try await session.visualClick("定位中央 E2E Project 项目卡片左上角的标题，点击进入项目画布；不要点击顶部工作空间导航或终端缩略图")
                if index == 0 {
                    for marker in ["DARKONE", "DARKTWO"] {
                        try await command(session, "/terminal")
                        try await session.type("printf '\\033[0m\(marker) ready\\n\\033[32mSUCCESS\\033[0m  \\033[31mERROR\\033[0m\\n'")
                        try await session.shortcut(key: 36, modifiers: [])
                    }
                }
                if index == 0 {
                    try await click(session, x: 98, y: 26)
                    try await click(session, x: 1413, y: 26)
                    var frame = try await session.capturePixels()
                    try await session.drag(local: CGPoint(x: 437, y: 400), to: CGPoint(x: 300, y: 400), on: frame)
                    frame = try await session.capturePixels()
                    try await session.drag(local: CGPoint(x: 908, y: 400), to: CGPoint(x: 1140, y: 400), on: frame)
                }
                let drawers = try await session.visualCheck([
                    VisualCriterion(id: "materials", text: "左右抽屉属于同一深色材质，壁纸色彩轻微透出；墨黑应呈中性黑灰，石墨灰较亮，暖曜黑带暖棕，月岩是矿物黑灰微紫，深海石是蓝黑灰青；画布外框和抽屉协调"),
                    VisualCriterion(id: "text", text: "运行概览、空态、想法分段和右侧工具文字均清晰；卡片标题栏有深色背景与柔白文字，不出现暗底黑字或残留浅色标题"),
                    VisualCriterion(id: "terminal", text: "顶部导航末项为 E2E Project，已进入项目画布而非所有项目总览；两张真实终端的 DARKONE / DARKTWO 正文均可见，SUCCESS 绿色与 ERROR 红色可分辨；采用\(terminalStyle)，正文和光标清楚，标题栏与内容面边界完整")
                ])
                try session.save(drawers, "\(id)-drawers.png")
                return "项目及终端内容真实可见，双抽屉材质经过逐项看图"
            }
            try await session.step("\(id)：焦点、选中、悬停、按下与禁用") {
                try await click(session, x: 1290, y: 84)
                try await session.type("theme-check")
                let focus = try await session.visualCheck([
                    VisualCriterion(id: "focus", text: "右侧地址输入显示 theme-check，文字与焦点描边清楚；左侧空想法输入的发送按钮明显是禁用态")
                ])
                try session.save(focus, "\(id)-input-focus.png")
                try await session.selectAll()
                try await session.deleteSelection()
                try await session.visualClick("定位左抽屉想法分段中的「已指派」，点击切换；位置可能随上方内容改变")
                let restFrame = try await session.capturePixels()
                try await session.hover(local: CGPoint(x: 720, y: 40), on: restFrame, dwell: 120_000_000)
                let selected = try await session.visualCheck([
                    VisualCriterion(id: "selected", text: "已指派分段选中背景与文字可辨，仍可区分未选中的待办和已完成")
                ])
                try session.save(selected, "\(id)-selected.png")
                let selectedTarget = try await session.visualLocate("定位左抽屉当前已选中的「已指派」中心，检查已选状态仍有交互反馈")
                try await session.hover(local: selectedTarget.point, on: selectedTarget.frame, dwell: 120_000_000)
                let selectedHover = try await session.visualCheck([
                    VisualCriterion(id: "selected-hover", text: "已选中的已指派分段悬停背景比此前普通选中态更亮，保持文字清晰和选中含义")
                ])
                try session.save(selectedHover, "\(id)-selected-hover.png")
                try await session.press(local: selectedTarget.point, on: selectedHover) {
                    let held = try await session.visualCheck([
                        VisualCriterion(id: "selected-pressed", text: "已选中的已指派分段在鼠标按住时比悬停更明显，仍可读，没有布局跳动")
                    ])
                    try session.save(held, "\(id)-selected-pressed.png")
                }
                try await session.visualClick("定位左抽屉想法分段中的「待办」，点击恢复选中待办")
                let hoverTarget = try await session.visualLocate("定位左抽屉想法分段中的「已指派」中心，供悬停检查；此步不点击")
                try await session.hover(local: hoverTarget.point, on: hoverTarget.frame, dwell: 120_000_000)
                let hover = try await session.visualCheck([
                    VisualCriterion(id: "hover", text: "指针所在已指派分段的悬停背景清楚，文字可读；可区分当前选中的待办分段")
                ])
                try session.save(hover, "\(id)-hover.png")
                let pressTarget = try await session.visualLocate("定位左抽屉想法分段中的「已指派」中心，供鼠标按住检查")
                try await session.press(local: pressTarget.point, on: pressTarget.frame) {
                    let pressed = try await session.visualCheck([
                        VisualCriterion(id: "pressed", text: "鼠标仍按下时，已指派分段有清晰的按压反馈，文字仍清楚，控件没有错位或消失")
                    ])
                    try session.save(pressed, "\(id)-pressed.png")
                }
                return "焦点、选中、hover、mouse-down 和空输入禁用均已在真实窗口核验"
            }
        }
        try await session.step("两款矿物主题：有内容的运行/审批状态") {
            try await command(session, "/new claude")
            try await Task.sleep(nanoseconds: 5_000_000_000)
            for (_, id) in themes where ["moonstone", "deep-petrol"].contains(id) {
                try await command(session, "/theme \(id)")
                let status = try await session.visualCheck([
                    VisualCriterion(id: "status", text: "左侧运行概览出现真实假 Agent 的运行/审批内容，状态胶囊有稳定深底和彩色清晰文字；不能仅看到空运行列表"),
                    VisualCriterion(id: "actions", text: "可见的批准/处理操作按钮与底色有清晰边界，文字可读，没有白字融入亮色按钮")
                ])
                try session.save(status, "\(id)-agent-status.png")
                try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: id)
            }
            return "假 CLI 经真实 Hook 生成状态，两主题逐一核验"
        }
    }

    // 1440×840、左右各300pt的固定验收夹具；只发送真实输入，后置状态仍逐项看图。
    private static func click(_ session: Session, x: CGFloat, y: CGFloat) async throws {
        let frame = try await session.capturePixels()
        try await session.click(local: CGPoint(x: x, y: y), on: frame)
        try await Task.sleep(nanoseconds: 700_000_000)
    }

    private static func command(_ session: Session, _ text: String) async throws {
        let frame = try await session.capturePixels()
        try await session.click(local: CGPoint(x: frame.windowFrame.width / 2, y: frame.windowFrame.height - 38), on: frame)
        try await session.selectAll()
        try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 900_000_000)
    }
}
