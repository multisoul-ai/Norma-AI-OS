import E2EKit
import Fixtures
import Foundation

/// Configuration-only checks on a real window. No microphone or model preparation.
@MainActor
enum SettingsVoiceCommandsVisual {
    static func run(_ session: Session) async throws {
        defer { session.stop() }
        for (index, configuration) in [("zh-Hans", "sapphire-night"), ("en", "ink-black")].enumerated() {
            let (language, theme) = configuration
            try await session.step("语音分组列表、统一来源与高级诊断 · \(language) / \(theme)") {
                if index == 0 {
                    try session.launch(language: language)
                    _ = try await session.capturePixels()
                }
                session.stop()
                let database = try session.databaseURL()
                try Catalog.suppressFeatureGuide(database: database)
                try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
                try Catalog.seedSetting(database: database, key: "voice.desktop.mode", value: "legacy")
                try session.launch(language: language)
                try await session.resize(width: 1280, height: 850)
                try await session.shortcut(key: 43, modifiers: [0x37])
                try await session.visualClick("点击设置左侧栏的语音 Voice 分类；不要点击快捷键或音效分类")
                let overview = try await session.visualCheck([
                    VisualCriterion(id: "grouped", text: "真实语音设置页保留语音指令、GPT-Live、豆包的外层模式入口，当前为语音指令；状态提示与配置分组清楚，没有重叠或裁切控件"),
                    VisualCriterion(id: "theme-language", text: "页面使用 \(language) 对应语言，\(theme) 的材质与文字对比清楚，所有用户提示可读")
                ])
                try session.save(overview, "\(index)-voice-grouped-overview.png")
                try await session.scroll(xFraction: 0.75, yFraction: 0.6, deltaY: -280)
                let sources = try await session.visualCheck([
                    VisualCriterion(id: "sources", text: "识别来源只有本地识别 Local recognition 与云端识别 Cloud recognition 两个产品入口；没有并列的本地精准第三入口"),
                    VisualCriterion(id: "input", text: "麦克风、权限状态与识别来源按紧凑分组列表排列，按钮清楚且互不遮挡")
                ])
                try session.save(sources, "\(index)-voice-sources.png")
                try await session.visualClick("点击云端识别 Cloud recognition 来源按钮，仅浏览配置")
                try await session.scroll(xFraction: 0.75, yFraction: 0.6, deltaY: -220)
                let cloud = try await session.visualCheck([
                    VisualCriterion(id: "cloud", text: "云端识别配置显示 Azure 区域和识别语言等表单；本地识别仍可切回浏览，没有自动启动识别、录音或通话")
                ])
                try session.save(cloud, "\(index)-voice-cloud.png")
                let provider = try VoiceConfigurationFixture.value(database: database, key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("浏览云端配置不得改运行来源") }
                try await session.visualClick("点击本地识别 Local recognition 来源按钮，返回本地配置")
                try await session.visualClick("点击本地引擎与诊断 Local engine and diagnostics 这一行右侧的显示 Show 按钮")
                try await session.visualClick("点击高级诊断中的 A1 预览与终稿 A1 preview and final 引擎按钮，仅浏览详情")
                let engine = try await session.visualCheck([
                    VisualCriterion(id: "advanced", text: "A1 作为本地来源下的高级引擎详情显示，提供检查本地文件操作；未显示 A1 已选择，也未开始模型准备或录音"),
                    VisualCriterion(id: "feedback", text: "当前浏览项、已选来源与可用操作有清晰状态反馈，\(theme) 下文字和按钮可辨")
                ])
                try session.save(engine, "\(index)-voice-local-diagnostics.png")
                let after = try VoiceConfigurationFixture.value(database: database, key: "voice.asr.provider")
                guard after == provider else { throw E2EError.product("浏览 A1 不得改变既有引擎或默认") }
                return "双来源与诊断已看图；浏览不改运行配置；无私人数据和模型调用"
            }
        }
    }
}
