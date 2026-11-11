import CoreGraphics
import E2EKit
import Foundation

/// 场景：设置 → 特效 → 语音光晕（模型看图）。默认经典；选星尘/光环/关闭写入 effects.voice_glow.style，
/// 撤销恢复；选关闭后选项置灰；重启后保持；语音页旧开关改为跳转入口。
/// 构造：空隔离数据库，中文界面，1440×870。数据库值由程序断言，画面由当前 Agent 逐项看图。
@MainActor
public enum SettingsVoiceGlow {
    public nonisolated static let name = "settings-voice-glow"
    private static let key = "effects.voice_glow.style"

    public static func run(_ session: Session) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        defer { session.stop() }
        try await session.step("打开设置 → 特效，滚到语音光晕") {
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1440, height: 870)
            _ = try await session.visualCheck([VisualCriterion(id: "boot", text: "Norma 主界面已显示，不是启动动画或空白窗口；若仍在加载请回答 wait")])
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("点击设置左侧栏「通用」分组里的「特效」一行，不要点其他入口")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -6000)
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 350)
            let top = try await session.visualCheck([
                VisualCriterion(id: "section", text: "页面中出现「语音光晕」标题，旁边有「所有项目共用」标签；其下有大预览。若尚未滚到请回答 wait"),
                VisualCriterion(id: "hero", text: "大预览是一张缩略窗口（壁纸背景加底部 Dock 胶囊），内容完整、未溢出预览框；预览框之外没有光效或遮罩画到文字上")
            ])
            try session.save(top, "01a-voice-glow-hero.png")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -300)
            let frame = try await session.visualCheck([
                VisualCriterion(id: "grid", text: "风格网格有关闭、经典、光环、声纹、星尘五张缩略图，每张都是缩略窗口，光效没有画到缩略图下方的名称或选项行上"),
                VisualCriterion(id: "default", text: "当前风格为「经典」，经典卡片有选中勾与「默认」字样")
            ])
            try session.save(frame, "01-voice-glow-default.png")
            return "分区可见，当前为经典"
        }
        try await session.step("选星尘：写库、提示、可撤销") {
            try await session.visualClick("点击「语音光晕」风格网格里名为「星尘」的缩略图中心，不要点卡片运行特效区域的卡片")
            try await SettingsSupport.prove(session, key: key, value: "stardust")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 260)
            let frame = try await session.visualCheck([
                VisualCriterion(id: "picked", text: "语音光晕的当前名称为「星尘」，星尘缩略图有选中勾；大预览中能看到发光颗粒或柔光；状态行提示已切换为星尘，并出现「撤销」按钮")
            ])
            try session.save(frame, "02-stardust.png")
            try await session.visualClick("点击语音光晕区块里的「撤销」按钮")
            try await SettingsSupport.prove(session, key: key, value: "classic")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -260)
            return "stardust 写入后撤销回 classic"
        }
        try await session.step("逐风格看大预览：光环与声纹") {
            for (label, id) in [("光环", "halo"), ("声纹", "ribbon")] {
                try await session.visualClick("点击语音光晕风格网格里名为「\(label)」的缩略图中心")
                try await SettingsSupport.prove(session, key: key, value: id)
                try await Task.sleep(nanoseconds: 3_000_000_000)
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "hero-\(id)", text: "语音光晕大预览里能看到「\(label)」的效果：光环为窗口四周的彩色柔光（不是只有一条白细线）；声纹为 Dock 上方的彩色声波丝带。预览未被光效完全遮住")
                ])
                try session.save(frame, "03-\(id).png")
            }
            return "光环与声纹预览已看图"
        }
        try await session.step("选关闭：选项置灰") {
            try await session.visualClick("点击语音光晕风格网格里名为「关闭」的缩略图中心")
            try await SettingsSupport.prove(session, key: key, value: "off")
            let frame = try await session.visualCheck([
                VisualCriterion(id: "off", text: "大预览没有任何光效；「强度」和「聆听与思考时也显示」两行明显置灰，状态按钮与「重播」不可用")
            ])
            try session.save(frame, "04-off.png")
            return "关闭后选项置灰"
        }
        try await session.step("选光环后重启，设置保持") {
            try await session.visualClick("点击语音光晕风格网格里名为「光环」的缩略图中心")
            try await SettingsSupport.prove(session, key: key, value: "halo")
            session.stop()
            try session.launch(language: "zh-Hans")
            try await SettingsSupport.prove(session, key: key, value: "halo")
            return "重启后 halo 保持"
        }
        try await session.step("语音页只剩跳转入口") {
            try await session.resize(width: 1440, height: 870)
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("点击设置左侧栏 Norma 分组中的「语音」一行")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -1500)
            try await session.visualClick("点击语音页底部折叠的「更多设置」一行，展开它")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -1500)
            let frame = try await session.visualCheck([
                VisualCriterion(id: "link", text: "语音页「语音光晕」一行显示「风格、强度与预览已移到 设置 → 特效」和「在特效中设置」按钮，不再有开关。若还没滚到请回答 wait")
            ])
            try session.save(frame, "05-voice-link.png")
            return "跳转入口可见"
        }
    }
}
