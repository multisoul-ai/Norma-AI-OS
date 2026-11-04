import E2EKit
import Fixtures
import Foundation

/// Apple 一致性：标题复用产品名称翻译键，保留现有层级和键盘入口，不增加动效。
@MainActor
public enum SettingsLiveI18n {
    public nonisolated static let name = "settings-live-i18n"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("准备无凭据的 GPT-Live 隔离设置") {
            try session.launch(language: "zh-Hans")
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in
                FileManager.default.fileExists(atPath: database.path)
            }
            session.stop()
            try Catalog.seedSetting(database: database, key: "voice.desktop.mode", value: "gpt_live")
            try Catalog.seedSetting(database: database, key: SettingsSupport.languageKey, value: "zh-CN")
            try Catalog.suppressFeatureGuide(database: database)
            return "只预置模式，不提供凭据或启动通话"
        }
        for (index, variant) in [
            ("zh-CN", "简体中文", "与 Norma 对话，语音操作当前项目。", "需要 API Key", "对话偏好", "未配置"),
            ("en", "English", "Talk with Norma and work on your current project by voice.", "API Key required", "Conversation preferences", "Not configured"),
            ("zh-CN", "简体中文", "与 Norma 对话，语音操作当前项目。", "需要 API Key", "对话偏好", "未配置"),
        ].enumerated() {
            let (language, label, description, required, preferences, placeholder) = variant
            try await session.step("\(language)：GPT-Live 标题与周边文案") {
                if index == 0 {
                    try session.launch(language: "zh-Hans")
                    try await session.resize(width: 1280, height: 900)
                    try await session.shortcut(key: 43, modifiers: [0x37])
                } else {
                    try await session.visualClick("点击设置左侧的基础设置 / Basic Settings 分类")
                    try await session.visualClick("点击界面语言 / Interface Language 中的「\(label)」单选项，切换当前 App 语言")
                    try await SettingsSupport.prove(session, key: SettingsSupport.languageKey, value: language)
                }
                try await session.visualClick("点击设置左侧的语音 / Voice 分类，进入语音设置")
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "title", text: "当前是 GPT-Live 语音设置，配置页顶部标题完整显示 GPT-Live，与上方模式名称一致；不显示 voice.live_name 等翻译键"),
                    VisualCriterion(id: "language", text: "标题下方说明为「\(description)」，Key 状态为「\(required)」，偏好标题为「\(preferences)」；相应语种文案完整可读，无混杂语言或裁切"),
                    VisualCriterion(id: "offline", text: "页面没有配置 API Key；没有连接 / Connect 或挂断 / End call 按钮；密钥输入框显示「\(placeholder)」，不显示静音或停止播报操作"),
                ])
                try session.save(frame, "\(index)-\(language)-live-i18n.png")
                return "同一进程核验标题、说明、配置空态；语言切换立即生效，未启动通话"
            }
        }
    }
}
