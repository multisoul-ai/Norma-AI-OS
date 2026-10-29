import E2EKit
import Fixtures
import Foundation

/// Configuration-only acceptance: no microphone, model load or cloud connection.
@MainActor
public enum SettingsA1Configuration {
    public nonisolated static let name = "settings-a1-configuration"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        for (index, theme) in ["sapphire-night", "paper-blue"].enumerated() {
            try await session.step("A1 可选入口与模式隔离 · \(theme)") {
                if index > 0 {
                    session.stop()
                    try VoiceConfigurationFixture.theme(database: session.databaseURL(), id: theme)
                }
                try session.launch(language: "zh-Hans")
                _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
                try await session.shortcut(key: 43, modifiers: [0x37])
                _ = try await session.wait(Expectation(anyOf: ["语音", "Voice"], timeout: 8))
                try await session.click(matching: ["语音", "Voice"], pick: .left)
                _ = try await session.wait(Expectation(anyOf: ["对话模式", "Conversation mode"], timeout: 8))
                try await session.click(matching: ["语音指令", "Voice Commands"], pick: .top)
                for _ in 0..<6 {
                    let frame = try await session.capture()
                    if frame.contains("本地精准") || frame.contains("On-device Accurate") { break }
                    try await session.scroll(pixels: -180)
                    try await Task.sleep(nanoseconds: 200_000_000)
                }
                try await session.click(matching: ["本地精准", "On-device Accurate"], pick: .top)
                let local = try await session.wait(Expectation(anyOf: ["检查本地文件", "Check local files"], absent: ["本地精准 · 已选择", "Accurate · Selected"], timeout: 8))
                let provider = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.asr.provider")
                guard provider == nil || provider == "local_sherpa" else { throw E2EError.product("浏览 A1 不得改变默认 provider: \(provider ?? "nil")") }
                try session.save(local, "\(index)-a1-options.png")
                try await session.scroll(pixels: 1200)
                _ = try await session.wait(Expectation(allOf: ["GPT-Live"], timeout: 5))
                try await session.click(matching: ["GPT-Live"], pick: .top)
                let live = try await session.wait(Expectation(anyOf: ["需要 API Key", "API Key required"], absent: ["检查本地文件", "Check local files"], timeout: 8))
                try session.save(live, "\(index)-gpt-live-isolated.png")
                try await session.click(matching: ["豆包", "Doubao 2.0 Voice"], pick: .top)
                let doubao = try await session.wait(Expectation(anyOf: ["豆包连接", "Doubao Connection"], absent: ["检查本地文件", "Check local files"], timeout: 8))
                try session.save(doubao, "\(index)-doubao-isolated.png")
                return "A1 只浏览未激活；默认 ASR 未改变；GPT-Live/豆包配置页保持独立，无模型或通话运行"
            }
        }
    }
}
