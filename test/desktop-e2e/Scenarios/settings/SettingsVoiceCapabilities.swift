import E2EKit
import Fixtures
import Foundation

@MainActor
enum SettingsVoiceCapabilities {
    static let a1Name = "settings-voice-a1-install"
    static let jevName = "settings-voice-jev-configuration"
    static let errorsName = "settings-voice-capability-errors-en"

    static func runEnglishErrors(_ session: Session) async throws {
        defer { session.stop() }
        try session.launch(language: "en")
        _ = try await session.capturePixels()
        session.stop()
        try seed(session, theme: "ink-black")
        try await session.step("英文A1安装失败反馈与恢复入口") {
            try await openVoice(session, language: "en", scroll: -480)
            try await session.visualClick("点击A1的Python 3.12 runtime路径输入框")
            try await session.selectAll()
            try await session.type("/norma-e2e-missing-python")
            try await session.visualClick("点击A1的Download and install按钮")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "a1-failure-en", text: "英文明确提示Python文件不存在或无法执行、选择Python 3.12后重试，Retry installation和路径输入框可用；没有中文错误、未翻译键或虚假的安装成功")
            ]), "a1-invalid-python-en.png")
            guard let data = session.dataRoot,
                  !FileManager.default.fileExists(atPath: data.appendingPathComponent("models/a1/runtime.json").path) else {
                throw E2EError.product("失败安装不得生成runtime.json")
            }
            return "英文实际无效Python失败，配置未写入，恢复入口可用"
        }
        try await session.step("英文Jev真实拒绝反馈与重试入口") {
            try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: -640)
            try await session.visualClick("点击Jev API Key输入框")
            try await session.type("norma-synthetic-invalid-key")
            try await session.visualClick("点击Jev的Verify connection按钮")
            try await Task.sleep(nanoseconds: 17_000_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "jev-failure-en", text: "英文显示真实HTTP 401或403并提示检查API Key后重试；密钥只显示掩码，Verify connection可重试，生产语音入口保持禁用，没有连接通过的假状态")
            ]), "jev-invalid-en.png")
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1") == nil else {
                throw E2EError.product("未保存的无效草稿不得持久化")
            }
            return "英文实际服务拒绝无效凭据，未保存配置"
        }
    }

    static func seed(_ session: Session, theme: String) throws {
        let database = try session.databaseURL()
        try Catalog.suppressFeatureGuide(database: database)
        try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
        try Catalog.seedSetting(database: database, key: "voice.desktop.mode", value: "legacy")
    }

    static func openVoice(_ session: Session, language: String, scroll: Int32) async throws {
        try session.launch(language: language)
        try await session.resize(width: 1280, height: 850)
        try await session.shortcut(key: 43, modifiers: [0x37])
        try await session.visualClick("点击设置左侧的语音 Voice 分类")
        try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: scroll)
    }

    static func runA1(_ session: Session) async throws {
        defer { session.stop() }
        let python = try VoiceCapabilitiesFixture.preparedPython()
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        session.stop()
        try seed(session, theme: "sapphire-night")
        guard let data = session.dataRoot, let output = session.output else { throw E2EError.env("缺少隔离根目录") }
        let manifest = data.appendingPathComponent("models/a1/runtime.json")
        try await session.step("A1无效Python明确失败并可编辑恢复") {
            try await openVoice(session, language: "zh-Hans", scroll: -480)
            try await session.visualClick("点击A1的Python 3.12运行环境路径输入框")
            try await session.selectAll()
            try await session.type("/norma-e2e-missing-python")
            try await session.visualClick("点击A1的下载安装或重试安装按钮，不点检查本地文件或启用A1")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "failure", text: "A1明确显示Python文件不存在或无法执行，并提示选择Python 3.12后重试；不能声称安装成功。输入框和可重试操作仍可用，未出现未翻译键或文字重叠")
            ]), "a1-invalid-python.png")
            guard !FileManager.default.fileExists(atPath: manifest.path) else { throw E2EError.product("失败安装不得保存runtime.json") }
            return "真实无效运行环境返回失败，未创建安装配置"
        }
        try await session.step("从页面下载14个固定模型文件并保存配置") {
            try await session.visualClick("点击A1的Python运行环境输入框以更正路径")
            try await session.selectAll()
            try await session.type(python)
            let log = output.appendingPathComponent("app.log")
            let offset = (try? Data(contentsOf: log).count) ?? 0
            try await session.visualClick("点击A1的下载安装或重试安装按钮")
            // Fresh dependencies and 1.21 GB of release files can exceed 15 minutes.
            // This is installation time, not the interactive voice latency budget.
            let deadline = Date().addingTimeInterval(2700)
            var nextCapture = Date()
            while !FileManager.default.fileExists(atPath: manifest.path) {
                try session.ensureRunning()
                if let bytes = try? Data(contentsOf: log), bytes.count > offset,
                   let recent = String(data: bytes.dropFirst(offset), encoding: .utf8),
                   recent.contains("A1 installation failed") {
                    try session.save(try await session.capturePixels(), "a1-install-failure.png")
                    throw E2EError.product("真实A1安装失败，请核对本次app.log和失败截图；不以模拟安装代替")
                }
                if Date() >= deadline { throw E2EError.env("真实依赖、模型下载或校验超过45分钟；没有完整安装证据") }
                if Date() >= nextCapture {
                    try session.save(try await session.capturePixels(), "a1-download-progress-\(Int(deadline.timeIntervalSinceNow)).png")
                    print("A1真实模型下载或校验仍在进行"); fflush(stdout)
                    nextCapture = Date().addingTimeInterval(20)
                }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            _ = try VoiceCapabilitiesFixture.verifyA1(dataRoot: data, output: output)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "installed", text: "A1显示模型文件与运行环境已安装并通过校验；说明启用时才加载模型，下载安装按钮恢复可操作，没有停留在下载中或假装已开始录音；中文状态和路径可读")
            ]), "a1-installed-zh.png")
            try VoiceCapabilitiesFixture.keepLog(output: output, name: "a1-install-zh.log")
            return "真实网络下载，14个文件逐一复查长度与SHA256，Python哈希匹配，未启用引擎"
        }
        try await session.step("英文重开A1配置并复查已有安装") {
            let previous = try Data(contentsOf: manifest)
            session.stop()
            try seed(session, theme: "ink-black")
            try await openVoice(session, language: "en", scroll: -480)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reload", text: "英文A1配置显示此前保存的Python路径与Saved A1 configuration found，不声称本次已做连接或识别；输入框和安装按钮可见，文案无重叠和未翻译键")
            ]), "a1-reopened-en.png")
            try await session.scroll(xFraction: 0.55, yFraction: 0.6, deltaY: -160)
            try await session.visualClick("点击A1的Check local files检查本地文件按钮，不点Activate A1")
            try await Task.sleep(nanoseconds: 5_000_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "files-ready", text: "A1文件校验结束后出现Activate A1操作，未显示文件缺失或完整性失败；只是文件已就绪，不能显示已录音或已识别")
            ]), "a1-rechecked-en.png")
            guard try Data(contentsOf: manifest) == previous else { throw E2EError.product("重开和只读检查不得改写安装配置") }
            let provider = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.asr.provider")
            guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("安装不能自动切换实际ASR") }
            _ = try VoiceCapabilitiesFixture.verifyA1(dataRoot: data, output: output)
            return "重开保留相同manifest，真实文件复查通过，provider未自动切换"
        }
    }

    static func runJev(_ session: Session) async throws {
        defer { session.stop() }
        try VoiceCapabilitiesFixture.requireJevCredential()
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        session.stop()
        try seed(session, theme: "sapphire-night")
        guard let data = session.dataRoot, let output = session.output else { throw E2EError.env("缺少隔离根目录") }
        try await session.step("Jev无效凭据由真实服务拒绝") {
            try await openVoice(session, language: "zh-Hans", scroll: -1120)
            try await session.visualClick("点击Jev API Key密钥输入框")
            try await session.type("norma-synthetic-invalid-key")
            try await session.visualClick("点击Jev的验证连接按钮")
            try await Task.sleep(nanoseconds: 17_000_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "invalid", text: "Jev显示真实HTTP 401或403凭据拒绝，并提示检查API Key后重试；生产语音入口保持禁用，输入框不显示明文密钥，没有错误地标为连接通过")
            ]), "jev-invalid-zh.png")
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1") == nil else { throw E2EError.product("未保存的无效草稿不能写入配置引用") }
            return "人工无效Key被真实服务拒绝；未持久保存"
        }
        try await session.step("Jev凭据实际保存到Keychain并真实验证") {
            try await session.visualClick("点击Jev的从环境变量导入按钮")
            try await session.visualClick("点击Jev的保存凭据按钮")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "saved", text: "凭据已保存在钥匙串并成功加载，明确本次尚未验证连接；输入框已清空或仅有掩码，不能出现真实密钥；保存与连接可用状态分开")
            ]), "jev-keychain-saved-zh.png")
            guard let reference = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1"), reference.hasPrefix("jev-") else { throw E2EError.product("保存后必须有非敏感的Keychain引用") }
            try VoiceCapabilitiesFixture.assertNoPlaintextKey(dataRoot: data)
            try await session.visualClick("点击Jev验证连接按钮，使用刚保存的凭据")
            try await Task.sleep(nanoseconds: 17_000_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "valid", text: "Jev连接验证通过，显示实际jev版本（不是jev-latest别名）和实测毫秒数；生产入口仍禁用，无录音、转写或派发成功的声称，中文布局无重叠")
            ]), "jev-verified-zh.png")
            try VoiceCapabilitiesFixture.keepLog(output: output, name: "jev-configuration-zh.log")
            return "Keychain实际保存重读，SQLite无明文，真实固定合成Choice验证通过"
        }
        try await session.step("英文重开加载Keychain、再次验证并清除测试配置") {
            let reference = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1")
            session.stop()
            try seed(session, theme: "ink-black")
            try await openVoice(session, language: "en", scroll: -1120)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reopened", text: "英文Jev配置已从Keychain加载且提示本次启动尚未验证连接；API Key不明文回填，生产入口保持禁用，固定端点和请求模型可读")
            ]), "jev-reopened-en.png")
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1") == reference else { throw E2EError.product("重开必须沿用同一安全存储引用") }
            try await session.visualClick("点击Jev的Verify connection按钮")
            try await Task.sleep(nanoseconds: 17_000_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reverified", text: "英文显示Connection verified、实际jev版本和耗时；密钥仍掩码，production voice entry remains disabled，未把配置成功当成生产启用")
            ]), "jev-reverified-en.png")
            try VoiceCapabilitiesFixture.assertNoPlaintextKey(dataRoot: data)
            try await session.visualClick("点击Jev的Clear credentials按钮清除本次隔离测试凭据")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "cleared", text: "Jev回到Not configured，密钥框空白，Verify connection禁用，生产语音入口仍禁用；没有残留连接通过的旧状态")
            ]), "jev-cleared-en.png")
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.jev.credential_ref.v1") == nil else { throw E2EError.product("清除后不可保留有效引用") }
            return "重开Keychain加载、真实重新验证及清除均有实际结果；生产未启用"
        }
    }
}
