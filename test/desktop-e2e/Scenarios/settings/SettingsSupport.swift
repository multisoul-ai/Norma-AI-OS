import CoreGraphics
import E2EKit
import Foundation

@MainActor
enum SettingsSupport {
    static let languageKey = "general.language.v1"
    static let energyKey = "performance.idle_animation_throttling.v1"

    static func open(_ session: Session) async throws {
        try session.launch()
        // 控制通道早于异步初始窗口开放；先复用只读 window 就绪屏障，再开始截图与输入。
        try await session.resize(width: 1280, height: 800)
        _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
        try await session.shortcut(key: 43, modifiers: [0x37])
        _ = try await session.wait(Expectation(anyOf: ["交流风格", "Communication Style"], timeout: 10))
    }

    static func select(_ session: Session, labels: [String], content: [String]) async throws -> Frame {
        // 目的地可能在侧栏滚动区下方；每次动作都重新读取窗口和 OCR。
        try await session.scroll(xFraction: 0.25, yFraction: 0.5, deltaY: 1200)
        for _ in 0..<6 {
            let frame = try await session.capture()
            var destination = frame.pick(labels, .left)?.center
            // Graphite 中文人格页的整图 OCR 会漏掉「主题」「壁纸」两字，真实截图中控件完整。
            // 用同列可见的「基础设置」与「特效」夹定中间行；点击后仍须验证壁纸页正文。
            if destination == nil, labels.contains("壁纸"),
               let basic = frame.pick(["基础设置"], .left), let effects = frame.pick(["特效"], .left),
               basic.center.x < frame.windowFrame.width * 0.35,
               abs(basic.center.x - effects.center.x) < 24,
               (100...160).contains(effects.center.y - basic.center.y) {
                destination = CGPoint(x: effects.center.x, y: basic.center.y + (effects.center.y - basic.center.y) * 2 / 3)
                try session.save(frame, "wallpaper-sidebar-anchor.png")
            }
            if let destination, destination.x < frame.windowFrame.width * 0.4 {
                try await session.click(local: destination, on: frame)
                let expected = Expectation(anyOf: content, timeout: 5)
                // 内容区滚动位置会跨页面保留，目标可能在当前视口上方或下方：先向一侧找，再回扫另一侧。
                for step in 0..<12 {
                    let page = try await session.capture()
                    if page.satisfies(expected) { return page }
                    try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: step < 4 ? -260 : 260)
                }
                return try await session.wait(expected)
            }
            try await session.scroll(xFraction: 0.25, yFraction: 0.5, deltaY: -180)
        }
        let missing = try await session.capture()
        try session.save(missing, "missing-settings-destination.png")
        throw E2EError.product("侧栏找不到目的地 \(labels)：\(missing.summary)")
    }

    static func basic(_ session: Session) async throws -> Frame {
        try await select(session, labels: ["基础设置", "Basic Settings"], content: ["界面语言", "Interface Language"])
    }

    static func language(_ session: Session, label: String, saved: String) async throws {
        _ = try await basic(session)
        try await session.click(matching: [label], pick: .left)
        try await prove(session, key: languageKey, value: saved)
    }

    static func prove(_ session: Session, key: String, value: String) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if try session.setting(key) == value { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("设置 \(key) 应保存为 \(value)，实际 \(try session.setting(key) ?? "nil")")
    }

    static func energy(_ session: Session, enabled: Bool) async throws -> Frame {
        for _ in 0..<5 {
            let frame = try await session.capture()
            let labels = ["Smoothest Response", "Energy Saver", "最流畅响应", "节能"]
            let badge = frame.texts.filter { text in
                labels.contains { text.value.localizedCaseInsensitiveCompare($0) == .orderedSame }
                    || text.value.hasPrefix("最流畅")
            }.max { $0.center.x < $1.center.x }
            if let badge, badge.center.x > frame.windowFrame.width * 0.4 {
                let width = frame.windowFrame.width
                let panel = min(max(width * 0.7, 760), min(1280, width - 64))
                // 1280×800 基准窗口：内容边距 20 + 卡片内边距 18 + 半个 Switch 18 = 56。
                let point = CGPoint(x: (width + panel) / 2 - 56, y: badge.center.y + 16)
                try await session.click(local: point, on: frame)
                try await prove(session, key: energyKey, value: enabled ? "true" : "false")
                return try await session.wait(Expectation(
                    anyOf: enabled ? ["Energy Saver", "节能"] : ["Smoothest Response", "最流畅"]
                ))
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -160)
        }
        throw E2EError.product("基础设置中找不到节能开关")
    }
}
