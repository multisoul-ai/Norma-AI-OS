import E2EKit
import Fixtures
import Foundation

/// 语音设置三种对话模式，在蓝宝石之夜和墨黑下看真实窗口。
/// 不保存 Key、不打开麦克风、不发起通话。
@MainActor
public enum VoiceSettingsModes {
    public nonisolated static let name = "voice-settings-modes"

    public static func run(_ session: Session) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        defer { session.stop() }
        // paper-blue 会迁移成 ink-black，现行目录没有浅色主题。第二套用墨黑，和蓝宝石之夜对照。
        for (index, appearance) in [("sapphire-night", "蓝宝石之夜"), ("ink-black", "墨黑")].enumerated() {
            let (theme, label) = appearance
            try await session.step("\(label)下打开语音指令") {
                session.stop()
                try session.launch(language: "zh-Hans")
                _ = try await session.capturePixels()
                session.stop()
                let database = try session.databaseURL()
                try Catalog.suppressFeatureGuide(database: database)
                try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
                try Catalog.seedSetting(database: database, key: "voice.desktop.mode", value: "legacy")
                try session.launch(language: "zh-Hans")
                try await session.resize(width: 1280, height: 850)
                try await session.shortcut(key: 43, modifiers: [0x37])
                try await session.visualClick("点击设置左侧栏 Norma 分组中的「语音」一行。不要点「快捷键」或「音效」")
                let top = try await session.visualCheck([
                    VisualCriterion(id: "mode", text: "语音指令为选中态（顶部分段条），能看到状态横幅与「语音输入」一组，每行左侧有圆形图标。\(label)下文字与背景对比清楚"),
                    VisualCriterion(id: "secret", text: "画面上没有明文 API Key，也没有把已保存的 Key 显示成可读字符")
                ])
                try session.save(top, "0\(index)-command-top-\(theme).png")
                // 语音输出在语音输入各行之下，默认面板高度装不下，向下滚一截再看。
                try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: -640)
                let page = try await session.visualCheck([
                    VisualCriterion(id: "groups", text: "能看到「语音输出」一组、「语音回复」一行，以及默认收起的「更多设置」（没有展开出长列表）。\(label)下文字与背景对比清楚"),
                    VisualCriterion(id: "secret", text: "画面上没有明文 API Key，也没有把已保存的 Key 显示成可读字符")
                ])
                try session.save(page, "0\(index)-command-\(theme).png")
                try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: 1200)
                return "\(label) 语音指令已看图"
            }
            try await session.step("\(label)下切到豆包 2.0") {
                try await session.visualClick("点击页面顶部分段条里的「豆包 2.0 语音」一格中心，不要点侧栏")
                let page = try await session.visualCheck([
                    VisualCriterion(id: "doubao", text: "豆包 2.0 语音为选中态，能看到火山方舟 API Key 的凭据行：未配置或已保存状态、保存按钮。\(label)下状态文字可读"),
                    VisualCriterion(id: "secret", text: "凭据行没有回显任何真实 Key 或一长串密钥字符")
                ])
                try session.save(page, "0\(index)-doubao-\(theme).png")
                let mode = try VoiceConfigurationFixture.value(database: try session.databaseURL(), key: "voice.desktop.mode")
                guard mode == "doubao" else { throw E2EError.product("豆包模式未写入，实际 \(mode ?? "空")") }
                return "\(label) 豆包已看图"
            }
            try await session.step("\(label)下切到 GPT-Live") {
                try await session.visualClick("点击页面顶部分段条里的「GPT-Live」一格中心，不要点侧栏")
                let page = try await session.visualCheck([
                    VisualCriterion(id: "live", text: "GPT-Live 为选中态，能看到 OpenAI API Key 的凭据行和保存按钮。\(label)下按钮与文字可辨，没有铺满的通话控制"),
                    VisualCriterion(id: "secret", text: "OpenAI 凭据行没有回显 Key")
                ])
                try session.save(page, "0\(index)-live-\(theme).png")
                let mode = try VoiceConfigurationFixture.value(database: try session.databaseURL(), key: "voice.desktop.mode")
                guard mode == "gpt_live" else { throw E2EError.product("GPT-Live 模式未写入，实际 \(mode ?? "空")") }
                return "\(label) GPT-Live 已看图"
            }
        }
    }
}
