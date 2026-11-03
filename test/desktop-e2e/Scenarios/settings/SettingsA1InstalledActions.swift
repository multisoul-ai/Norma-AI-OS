import E2EKit
import Fixtures
import Foundation

@MainActor
enum SettingsA1InstalledActions {
    static let name = "settings-voice-a1-installed-actions"

    static func run(_ session: Session) async throws {
        defer { session.stop() }
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        session.stop()
        guard let data = session.dataRoot, let output = session.output else { throw E2EError.env("缺少隔离目录") }
        try InstalledA1Fixture.clone(into: data)
        _ = try VoiceCapabilitiesFixture.verifyA1(dataRoot: data, output: output)
        let manifest = data.appendingPathComponent("models/a1/runtime.json")
        let previous = try Data(contentsOf: manifest)
        let packages = data.appendingPathComponent("models/a1/packages")
        let packageNames = try FileManager.default.contentsOfDirectory(atPath: packages.path).sorted()
        for (language, theme) in [("zh-Hans", "sapphire-night"), ("en", "paper-blue")] {
            try await session.step("已安装状态、明确重装与未启用提示 · \(language)") {
                session.stop()
                try SettingsVoiceCapabilities.seed(session, theme: theme)
                try await SettingsVoiceCapabilities.openVoice(session, language: language, scroll: -480)
                try await session.visualClick("点击A1的检查本地文件 Check local files按钮；不要点重新安装 Reinstall")
                try await Task.sleep(nanoseconds: 12_000_000_000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "installed", text: "A1显示明确的已安装 Installed状态，原下载按钮已禁用为已安装，另有独立的重新安装 Reinstall入口；不能仍把主操作标为下载安装 Download & install")
                ]), "installed-actions-\(language).png")
                try await session.visualClick("点击A1灰色禁用的已安装 Installed按钮，不点重新安装或使用本地精准")
                try await Task.sleep(nanoseconds: 1_000_000_000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "no-redownload", text: "点击已安装后仍保持安装成功，没有下载进度、取消安装或失败状态；明确提示已安装但尚未启用，并提供使用本地精准 Use Accurate按钮，不能声称已加载模型或已录音")
                ]), "installed-no-redownload-\(language).png")
                guard try Data(contentsOf: manifest) == previous,
                      try FileManager.default.contentsOfDirectory(atPath: packages.path).sorted() == packageNames else {
                    throw E2EError.product("已安装状态操作不能改写配置、创建下载暂存或重新下载模型")
                }
                let provider = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("安装状态验收不能隐式激活A1") }
                return "真实模型文件复用；已安装主操作不下载，重装明确分开，启用状态不混淆"
            }
        }
    }
}
