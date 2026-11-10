import CoreGraphics
import E2EKit
import Foundation

/// 场景：设置 → 特效 → 语音光晕。选风格写入全局设置并提示，撤销恢复，
/// 选「关闭」后选项禁用，重启后保持；语音页的旧开关已改为跳转入口。
/// 构造：空隔离数据库；英文界面（本机 OCR 读不了中文）。
/// 预期：默认经典；点 Stardust → effects.voice_glow.style=stardust 且状态行提示；撤销 → classic；
/// 点 Halo → halo；点 Off → off；点 Halo 后重启，设置仍为 halo；
/// 语音页出现「Open in Effects」入口，旧开关文案不再出现。
@MainActor
public enum SettingsVoiceGlow {
    public nonisolated static let name = "settings-voice-glow"
    private static let key = "effects.voice_glow.style"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("英文界面，打开特效页的语音光晕分区") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Choose the animation", "Playback"])
            let frame = try await scrollTo(session, ["Voice glow"])
            try session.save(frame, "01-voice-glow-default.png")
            guard frame.contains("Classic") else {
                throw E2EError.product("默认应显示 Classic：\(frame.summary)")
            }
            return "分区可见，当前为 Classic"
        }
        try await session.step("点 Stardust：写入全局设置并提示，可撤销") {
            try await pickTile(session, "Stardust")
            try await SettingsSupport.prove(session, key: key, value: "stardust")
            let frame = try await scrollTo(session, ["Voice glow changed"])
            try session.save(frame, "02-stardust-picked.png")
            try await clickScrolling(session, "Undo")
            try await SettingsSupport.prove(session, key: key, value: "classic")
            return "stardust 写入后撤销回 classic"
        }
        try await session.step("点 Halo、点 Off：选项随之禁用") {
            try await pickTile(session, "Halo")
            try await SettingsSupport.prove(session, key: key, value: "halo")
            try await pickTile(session, "Off")
            try await SettingsSupport.prove(session, key: key, value: "off")
            let frame = try await scrollTo(session, ["Intensity"])
            try session.save(frame, "03-off-options-disabled.png")
            return "off 写入，选项区已置灰"
        }
        try await session.step("点 Halo 后重启，设置保持") {
            try await pickTile(session, "Halo")
            try await SettingsSupport.prove(session, key: key, value: "halo")
            session.stop()
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 25))
            try await SettingsSupport.prove(session, key: key, value: "halo")
            return "重启后 effects.voice_glow.style=halo"
        }
        try await session.step("语音页只剩跳转入口") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(allOf: ["Settings", "Effects"], timeout: 10))
            _ = try await SettingsSupport.select(session, labels: ["Voice"], content: ["Spoken replies", "Conversation"])
            let frame = try await session.capture()
            try session.save(frame, "04-voice-page-link.png")
            guard frame.contains("Open in Effects") || frame.contains("Style, intensity and preview moved") else {
                throw E2EError.product("语音页应有「Open in Effects」入口：\(frame.summary)")
            }
            return "语音页显示跳转入口"
        }
    }

    /// 从页面顶部往下滚，直到出现任一文字。
    private static func scrollTo(_ session: Session, _ labels: [String]) async throws -> Frame {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 6000)
        for _ in 0..<40 {
            let frame = try await session.capture()
            if labels.contains(where: { frame.contains($0) }) { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -240)
        }
        throw E2EError.product("特效页找不到 \(labels)")
    }

    /// 风格网格里点某一格：名称在缩略图下方，点名称上方 40px 的缩略图。
    private static func pickTile(_ session: Session, _ label: String) async throws {
        _ = try await scrollTo(session, ["Voice glow"])
        for _ in 0..<12 {
            let frame = try await session.capture()
            if let title = frame.pick(["Voice glow"], .bottom),
               let cell = frame.texts.filter({ $0.value == label && $0.center.y > title.center.y }).max(by: { $0.center.y < $1.center.y }) {
                try await session.click(local: CGPoint(x: cell.center.x, y: cell.center.y - 40), on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("风格网格找不到 \(label)")
    }

    private static func clickScrolling(_ session: Session, _ label: String) async throws {
        _ = try await scrollTo(session, ["Voice glow"])
        for _ in 0..<12 {
            let frame = try await session.capture()
            if let target = frame.texts.first(where: { $0.value == label && $0.center.x > frame.windowFrame.width * 0.3 }) {
                try await session.click(local: target.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("找不到按钮 \(label)")
    }
}
