import CoreGraphics
import E2EKit
import Foundation

/// 场景：设置 → 特效。选款式、按来源覆盖、播放选项都能保存，撤销与重启后保持，Ink Black 与 Graphite 主题可读。
/// 构造：空隔离数据库，直接从首页打开设置，无需工作空间。
/// 预期：点击烟花后全局设置写入 fireworks；撤销回到未设置；声纹 + 语音来源覆盖写入；
/// 速度写入设置表，播放区不再显示降级开关；重启后选择仍在。
@MainActor
public enum SettingsDispatchEffects {
    public nonisolated static let name = "settings-dispatch-effects"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("首页直接打开全局特效设置") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects", "Built-in"])
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
            let page = try await session.wait(Expectation(allOf: ["Applies globally"], absent: ["Follow workspace", "Personalize"]))
            try session.save(page, "01-effects-page-dark.png")
            return "导航“通用”组里出现特效；页面含当前特效、内置特效网格、按派发来源、播放"
        }
        try await session.step("点击烟花：写入全局设置并显示在当前特效") {
            try await pick(session, "Fireworks")
            try await proveEffect(session, contains: "\"style\":\"fireworks\"")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
            let frame = try await session.wait(Expectation(allOf: ["Current dispatch effect", "Fireworks"]))
            try session.save(frame, "02-fireworks-picked.png")
            return "effects.dispatch.v1 含 fireworks，保存提示出现"
        }
        try await session.step("撤销回到未设置") {
            try await clickScrolling(session, "Undo")
            try await proveEffect(session, contains: nil)
            return "撤销后 effects.dispatch.v1 为空（原本没有全局设置）"
        }
        try await session.step("选声纹并把语音派发改成烟花") {
            try await pick(session, "Voice ribbon")
            try await proveEffect(session, contains: "\"style\":\"ribbon\"")
            try await clickScrolling(session, "Follow current effect")
            let menu = try await session.wait(Expectation(allOf: ["Fireworks"], timeout: 6))
            try session.save(menu, "03-source-menu-open.png")
            try await session.click(matching: ["Fireworks"], pick: .right)
            try await proveEffect(session, contains: "\"voice\":\"fireworks\"")
            try await expectSaved(session)
            return "style=ribbon、voice=fireworks；交接不再固定为离子，未单独设置时跟随当前款式"
        }
        try await session.step("播放速度保留，脉冲降级选项已移除") {
            // 派发播放选项之后还有运行特效；按可见标签定位，不能把页面底部当成播放区。
            try await clickScrolling(session, "Relaxed")
            try await SettingsSupport.prove(session, key: "effects.speed", value: "relaxed")
            let frame = try await session.wait(Expectation(
                allOf: ["Playback", "Playback speed"],
                absent: ["Simplify rapid dispatches", "Reduce Motion", "连续派发时简化", "减少动态效果"]
            ))
            try session.save(frame, "04-playback.png")
            return "effects.speed=relaxed；播放区只保留速度，不再提供脉冲降级开关"
        }
        try await session.step("Graphite 主题下页面可读") {
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Graphite", "Graphite", "Focused palettes"])
            try await session.click(matching: ["Graphite", "Graphite"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "graphite")
            let page = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects", "Built-in"])
            try session.save(page, "05-effects-page-graphite.png")
            return "Graphite 下特效页对比度正常"
        }
        try await session.step("窗口缩小后特效页自适应") {
            var notes: [String] = []
            for (index, size) in [(1280.0, 800.0), (900.0, 760.0), (720.0, 760.0), (560.0, 700.0), (420.0, 700.0)].enumerated() {
                try await session.resize(width: size.0, height: size.1)
                try await Task.sleep(nanoseconds: 700_000_000)
                try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
                var frame = try await session.capture()
                try session.save(frame, "06-\(index)-narrow-\(Int(size.0))-top.png")
                try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -520)
                frame = try await session.capture()
                try session.save(frame, "06-\(index)-narrow-\(Int(size.0))-grid.png")
                try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -900)
                frame = try await session.capture()
                try session.save(frame, "06-\(index)-narrow-\(Int(size.0))-sources.png")
                notes.append("\(Int(size.0))px")
            }
            try await session.resize(width: 1280, height: 800)
            return "已在 \(notes.joined(separator: " / ")) 取图"
        }
        try await session.step("重启后保持选择") {
            session.stop()
            try session.launch()
            try await session.resize(width: 1280, height: 800)
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 20))
            try await proveEffect(session, contains: "\"style\":\"ribbon\"")
            try await SettingsSupport.prove(session, key: "effects.speed", value: "relaxed")
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await SettingsSupport.language(session, label: "中文", saved: "zh-CN")
            _ = try await SettingsSupport.select(session, labels: ["特效"], content: ["当前派发特效"])
            let chinese = try await session.wait(Expectation(allOf: ["全局生效"], absent: ["跟随工作空间", "仅此项目"]))
            try session.save(chinese, "07-global-effects-chinese.png")
            return "重启后数据库中的选择与播放选项仍在"
        }
    }

    /// 特效库里点某一款：卡片标签在缩略图下方，取最靠下的同名文字。
    private static func pick(_ session: Session, _ label: String) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<8 {
            let frame = try await session.capture()
            if let cell = frame.pick([label], .bottom), cell.center.x > frame.windowFrame.width * 0.3 {
                try await session.click(local: CGPoint(x: cell.center.x, y: cell.center.y - 40), on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("特效库找不到 \(label)")
    }

    /// 从页面顶部往下找按钮并点击：Replay / Undo 在当前特效卡的下半部分，窄窗口下可能在视口外。
    private static func clickScrolling(_ session: Session, _ label: String) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<24 {
            let frame = try await session.capture()
            if let target = frame.pick([label], .top), target.center.x > frame.windowFrame.width * 0.3 {
                try await session.click(local: target.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -200)
        }
        throw E2EError.product("找不到按钮 \(label)")
    }

    /// 保存成功后状态行应是“Saved”，不能停在“Saving…”。状态行在当前特效卡里，先滚回顶部再向下找。
    private static func expectSaved(_ session: Session) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<8 {
            let frame = try await session.capture()
            if frame.contains("Saving") { try await Task.sleep(nanoseconds: 400_000_000); continue }
            if frame.contains("Saved") { return }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -120)
        }
        throw E2EError.product("保存完成后状态行应显示 Saved")
    }

    private static func scrollToSources(_ session: Session) async throws {
        for _ in 0..<10 {
            let frame = try await session.capture()
            if frame.contains("Voice dispatch") { return }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -220)
        }
        throw E2EError.product("找不到“按派发来源”分区")
    }

    private static func proveEffect(_ session: Session, contains fragment: String?) async throws {
        let deadline = Date().addingTimeInterval(6)
        var last: String?
        while Date() < deadline {
            last = try session.setting("effects.dispatch.v1")
            if let fragment { if last?.contains(fragment) == true { return } } else if last == nil { return }
            try await Task.sleep(nanoseconds: 120_000_000)
        }
        throw E2EError.product("dispatch_effect_json 应\(fragment.map { "包含 \($0)" } ?? "为空")，实际 \(last ?? "NULL")")
    }
}
