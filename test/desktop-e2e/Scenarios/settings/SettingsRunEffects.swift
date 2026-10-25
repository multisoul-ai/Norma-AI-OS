import CoreGraphics
import E2EKit
import Foundation

/// 场景：设置 → 特效 → 卡片运行特效。选风格写入全局设置并立即作用于画布，撤销恢复，
/// 预览可固定状态，浅深主题可读，重启后保持。
/// 构造：空隔离数据库；英文界面（本机 OCR 读不了中文）；控制通道 `run_effects_fixture`
/// 在宿主窗口摆出六张不同运行状态的 Agent 卡片，作为“立即生效”的对照。
/// 预期：点灵光 → effects.run_style=aura 且状态行提示；撤销 → classic；点磁流体 → ferrofluid；
/// 预览固定“思考”时持续变化、固定“失败”平滑收敛后定格；关闭设置后画布上失败卡片周围与选择前明显不同；
/// 重启后设置仍为 ferrofluid，当前名称显示 Ferrofluid。
@MainActor
public enum SettingsRunEffects {
    public nonisolated static let name = "settings-run-effects"
    /// 与 `app/run_effects_fixture.rs` 的排布一致：第 6 张（失败）卡片。
    private static let failedCard = CGRect(x: 870, y: 390, width: 360, height: 230)

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        var before: CGImage?
        try await session.step("英文界面，画布摆出六张运行中的卡片（经典）") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            try await session.shortcut(key: 53, modifiers: [])
            try await session.installRunEffectsFixture()
            _ = try await session.wait(Expectation(anyOf: ["Run 6", "Run6"], absent: ["Basic Settings", "Playback speed"], timeout: 12))
            try await Task.sleep(nanoseconds: 1_800_000_000)
            let image = try await session.snapshot()
            try session.saveSnapshot(image, "01-canvas-classic.png")
            before = image
            return "画布上六张卡片，风格为默认经典"
        }
        try await session.step("打开特效页，找到卡片运行特效分区") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(allOf: ["Settings", "Effects"], timeout: 10))
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Choose the animation", "Playback"])
            let frame = try await scrollTo(session, ["Agent card run effects"])
            try session.save(frame, "02-run-section-dark.png")
            guard frame.contains("Classic") else {
                throw E2EError.product("分区应显示当前风格 Classic：\(frame.summary)")
            }
            return "分区可见，当前为 Classic"
        }
        try await session.step("点灵光：写入全局设置并提示，可撤销") {
            try await pickTile(session, "Aura")
            try await SettingsSupport.prove(session, key: "effects.run_style", value: "aura")
            let frame = try await scrollTo(session, ["Run effect changed"])
            try session.save(frame, "03-aura-picked.png")
            try await clickScrolling(session, "Undo")
            try await SettingsSupport.prove(session, key: "effects.run_style", value: "classic")
            return "aura 写入后撤销回 classic"
        }
        try await session.step("点磁流体，预览固定思考在动、固定失败定格") {
            try await pickTile(session, "Ferrofluid")
            try await SettingsSupport.prove(session, key: "effects.run_style", value: "ferrofluid")
            try await clickScrolling(session, "Thinking")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await scrollTo(session, ["Ferrofluid"])
            guard let hero = frame.pick(["Click the preview card"], .top) else {
                throw E2EError.product("找不到大预览旁的说明文字：\(frame.summary)")
            }
            // 大预览在说明文字左侧（宽 220–300px）：只取预览本身，避开设置面板外仍在动的画布。
            let area = CGRect(x: hero.rect.minX - 290, y: hero.center.y - 130, width: 270, height: 240)
            let a = try await session.snapshot()
            try await Task.sleep(nanoseconds: 300_000_000)
            let b = try await session.snapshot()
            let moving = changedPixels(a, b, in: area, windowSize: frame.windowFrame.size, threshold: 60)
            guard moving > 20 else {
                throw E2EError.product("固定“思考”时预览应持续变化，300ms 内只有 \(moving)px")
            }
            try session.saveSnapshot(b, "04-thinking-pinned.png")
            try await clickScrolling(session, "Failed")
            // 下垂平滑时间常数 450ms，收敛到 0.2% 约需 450×ln(500)≈2.8s，之后停帧。
            try await Task.sleep(nanoseconds: 3_500_000_000)
            let c = try await session.snapshot()
            try await Task.sleep(nanoseconds: 300_000_000)
            let d = try await session.snapshot()
            let still = changedPixels(c, d, in: area, windowSize: frame.windowFrame.size, threshold: 60)
            guard still <= 12 else {
                throw E2EError.product("固定“失败”3.5s 后预览应定格，实际 \(still)px 变化")
            }
            try session.saveSnapshot(d, "05-failed-pinned.png")
            return "思考 \(moving)px 变化；失败定格 \(still)px"
        }
        try await session.step("关闭设置：画布卡片立即换成磁流体") {
            try await session.shortcut(key: 53, modifiers: [])
            let frame = try await session.wait(Expectation(anyOf: ["Run 6", "Run6"], absent: ["Basic Settings", "Playback speed"], timeout: 8))
            try await Task.sleep(nanoseconds: 1_800_000_000)
            let after = try await session.snapshot()
            try session.saveSnapshot(after, "06-canvas-ferrofluid.png")
            guard let before else { throw E2EError.env("缺少经典画布基线") }
            let difference = changedPixels(before, after, in: failedCard.insetBy(dx: -24, dy: -24), windowSize: frame.windowFrame.size, threshold: 60)
            guard difference > 150 else {
                throw E2EError.product("关闭设置后失败卡片周围与经典只差 \(difference)px，风格没有立即生效")
            }
            return "无需重启，失败卡片周围差 \(difference)px"
        }
        try await session.step("浅色主题下分区可读") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(allOf: ["Settings", "Effects"], timeout: 10))
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Paper Blue", "aper Blue", "Bright palettes"])
            try await session.click(matching: ["Paper Blue", "aper Blue"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "paper-blue")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Choose the animation", "Playback"])
            let frame = try await scrollTo(session, ["Agent card run effects"])
            try session.save(frame, "07-run-section-light.png")
            return "Paper Blue 下分区截图已保存"
        }
        try await session.step("重启后保持") {
            session.stop()
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 25))
            try await SettingsSupport.prove(session, key: "effects.run_style", value: "ferrofluid")
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(allOf: ["Settings", "Effects"], timeout: 10))
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Choose the animation", "Playback"])
            let frame = try await scrollTo(session, ["Agent card run effects"])
            guard frame.texts.contains(where: { $0.value.replacingOccurrences(of: " ", with: "") == "Ferrofluid" && $0.center.x > frame.windowFrame.width * 0.3 }) else {
                throw E2EError.product("重启后当前风格应为 Ferrofluid：\(frame.summary)")
            }
            try session.save(frame, "08-after-restart.png")
            return "重启后 effects.run_style=ferrofluid，分区显示 Ferrofluid"
        }
    }

    /// 从页面顶部往下滚，直到出现任一文字。
    private static func scrollTo(_ session: Session, _ labels: [String]) async throws -> Frame {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<30 {
            let frame = try await session.capture()
            if labels.contains(where: { frame.contains($0) }) { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -240)
        }
        throw E2EError.product("特效页找不到 \(labels)")
    }

    /// 风格网格里点某一格：名称在缩略图下方，点名称上方 40px 的缩略图。
    private static func pickTile(_ session: Session, _ label: String) async throws {
        _ = try await scrollTo(session, ["Styles"])
        for _ in 0..<10 {
            let frame = try await session.capture()
            if let styles = frame.pick(["Styles"], .bottom),
               let cell = frame.texts.filter({ $0.value == label && $0.center.y > styles.center.y }).max(by: { $0.center.y < $1.center.y }) {
                try await session.click(local: CGPoint(x: cell.center.x, y: cell.center.y - 40), on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("风格网格找不到 \(label)")
    }

    private static func clickScrolling(_ session: Session, _ label: String) async throws {
        _ = try await scrollTo(session, ["Agent card run effects"])
        for _ in 0..<12 {
            let frame = try await session.capture()
            // 状态芯片前有色点，OCR 常把它读成 "• Failed"；按结尾匹配短文本。
            let target = frame.texts.first {
                $0.value.hasSuffix(label) && $0.value.count <= label.count + 2 && $0.center.x > frame.windowFrame.width * 0.3
            }
            if let target {
                try await session.click(local: target.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("找不到按钮 \(label)")
    }
}
