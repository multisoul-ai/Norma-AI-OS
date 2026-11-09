import E2EKit
import Fixtures
import Foundation

/// Apple 反馈原则：沿用组件与布局，明确首次下载、离线使用和无需配置Python，不增加动效。
@MainActor
public enum SettingsA1RuntimeDownload {
    public nonisolated static let name = "settings-a1-runtime-download"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        for (index, variant) in [("zh-Hans", "sapphire-night"), ("en", "graphite")].enumerated() {
            let (language, theme) = variant
            try await session.step("A1按需安装说明 · \(language) / \(theme)") {
                if index == 0 {
                    try session.launch(language: language)
                    _ = try await session.capturePixels()
                }
                session.stop()
                try SettingsVoiceCapabilities.seed(session, theme: theme)
                try await SettingsVoiceCapabilities.openVoice(session, language: language, scroll: -280)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "download", text: "本地精准A1 / Accurate local A1 分组明确说明语音运行环境与模型会自动下载、校验后安装，安装后可离线使用；没有声称运行环境内置于应用"),
                    VisualCriterion(id: "no-python", text: "说明明确无需配置Python，页面没有Python路径输入框或选择器；下载安装 / Download and install 按钮可用，未显示已安装、正在录音或已启用的假状态"),
                    VisualCriterion(id: "readable", text: "\(language) 文案与按钮完整可读，\(theme) 下对比清楚，没有遮挡、裁切或未翻译键")
                ]), "a1-runtime-\(language).png")
                let provider = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else {
                    throw E2EError.product("浏览下载安装说明不得自动切换A1")
                }
                guard let data = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
                guard !FileManager.default.fileExists(atPath: data.appendingPathComponent("models/a1/engine-downloads").path),
                      !FileManager.default.fileExists(atPath: data.appendingPathComponent("models/a1/runtime.json").path) else {
                    throw E2EError.product("仅浏览不得静默下载环境或发布模型配置")
                }
                return "按需下载说明和操作状态已看图；只浏览不安装，不切换来源"
            }
        }
    }
}
