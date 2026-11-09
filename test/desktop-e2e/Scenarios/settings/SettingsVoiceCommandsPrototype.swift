import E2EKit
import Fixtures
import Foundation

/// Real settings window only. Typed preview examples are not microphone evidence.
@MainActor
enum SettingsVoiceCommandsPrototype {
    static let name = "settings-voice-command-grouped"

    static func run(_ session: Session) async throws {
        defer { session.stop() }
        for (index, configuration) in [("zh-Hans", "sapphire-night"), ("en", "ink-black")].enumerated() {
            let (language, theme) = configuration
            try await session.step("原型分组与本地预览 · \(language) / \(theme)") {
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
                try await session.visualClick("点击设置左侧的语音 Voice 分类")
                let overview = try await session.visualCheck([
                    VisualCriterion(id: "columns", text: "宽窗口中，语音输入01分组位于左列，试一句指令 Try a command 独立位于右列；不是整页安装向导；无重叠、裁切或字段串列"),
                    VisualCriterion(id: "truthful", text: "右侧明确是本地规则预览，初始等待输入；没有固定Bruno/Henry、模拟回执或真实派发成功的假状态"),
                    VisualCriterion(id: "language", text: "所有文案按 \(language) 显示，当前主题 \(theme) 文字与控件清楚；未显示未翻译的voice.*键")
                ])
                try session.save(overview, "\(index)-grouped-overview.png")
                if index == 0 {
                    try await session.visualClick("点击右列试一句指令中的多行输入框")
                    try await session.type("测试员，请继续刚才的任务")
                    try await session.visualClick("点击右列的查看理解结果按钮")
                    try session.save(try await session.visualCheck([
                        VisualCriterion(id: "preview", text: "预览表明需要主模型进一步理解、先询问目标；没有把测试员替换为画布成员，没有任务派发成功或模型已调用的声称")
                    ]), "preview-unresolved.png")
                    try await session.visualClick("再次点击右列的指令输入框")
                    try await session.selectAll()
                    try await session.type("新的指令，先别执行")
                    try session.save(try await session.visualCheck([
                        VisualCriterion(id: "invalidated", text: "修改原话后，旧预览失效并显示等待预览；没有沿用上一句的结果或执行任务")
                    ]), "preview-invalidated.png")
                }
                try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: -370)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "groups", text: "滚动后可见理解与目标02、任务派发03或语音回复04；它们是同一左列连续分组列表，不是独立安装步骤。标题、内容与按钮未重叠")
                ]), "\(index)-grouped-details.png")
                try await session.resize(width: 640, height: 700)
                try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -1600)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "narrow", text: "窄窗口中预览移到分组列表下方，文字和输入框可读，没有左右两列挤压或横向越界；可滚动到预览底部，并如实显示当前画布没有可接收指令的成员")
                ]), "\(index)-narrow-preview.png")
                let provider = try VoiceConfigurationFixture.value(database: database, key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("预览不得改变识别provider") }
                guard try VoiceConfigurationFixture.value(database: database, key: "voice.desktop.mode") == "legacy" else { throw E2EError.product("预览不得改变对话模式") }
                return "四组列表、双列/窄屏与预览状态已看图；识别provider未改变；没有录音或真实派发"
            }
        }
    }
    static let sectionsName = "settings-voice-command-sections"

    static func runSections(_ session: Session) async throws {
        defer { session.stop() }
        for (index, configuration) in [("zh-Hans", "sapphire-night"), ("en", "ink-black")].enumerated() {
            let (language, theme) = configuration
            try await session.step("模型空态、回复分组与空编辑器 · \(language)") {
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
                try await session.visualClick("点击设置侧栏的语音 Voice 分类")
                try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: -370)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "model", text: "02理解与目标的当前模型 Current model 显示未配置 Not configured，不能空白；配置按钮、空画布成员摘要和03派发策略完整可读")
                ]), "\(index)-voice-model-state.png")
                try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: -420)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "reply", text: "04语音回复 Spoken replies 分组真实可见，保留未配置声音提示、更换声音按钮、任务播报和语音光晕开关；标题、控件与相邻分组未重叠，没有假装已配置或已播放")
                ]), "\(index)-voice-reply-group.png")
                try await session.resize(width: 640, height: 700)
                try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -10000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "empty-input", text: "空预览编辑器在窄屏仍为完整宽度，输入一句指令 Type a command 占位可读，不能塌成竖条；预览按钮和结果分隔线填充预览列，空画布说明可读且无横向溢出")
                ]), "\(index)-voice-narrow-empty.png")
                let provider = try VoiceConfigurationFixture.value(database: database, key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("配置浏览不得更改实际识别provider") }
                guard try VoiceConfigurationFixture.value(database: database, key: "voice.desktop.mode") == "legacy" else { throw E2EError.product("配置浏览不得改变运行模式") }
                return "已核验模型空态、04回复分组和空输入窄屏；没有录音、模型调用或派发"
            }
        }
    }

}
