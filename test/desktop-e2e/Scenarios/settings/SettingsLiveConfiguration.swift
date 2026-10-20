import E2EKit
import Fixtures
import Foundation

@MainActor
public enum SettingsLiveConfiguration {
    public nonisolated static let name = "settings-live-configuration"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("打开独立语音设置并选择GPT-Live") {
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await openVoice(session)
            try await session.click(matching: ["GPT-Live"], pick: .top)
            let live = try await session.wait(Expectation(anyOf: ["需要 API Key", "API Key required"], allOf: ["GPT-Live"], absent: ["刷新核对", "Refresh reconciliation", "静音", "Mute", "停止播报", "Stop speaking"], timeout: 8))
            try await waitValue(session, "voice.desktop.mode", "gpt_live")
            try session.save(live, "01-live-dark.png")
            return "GPT-Live已从UI选择，隔离SQLite模式=gpt_live"
        }
        try await session.step("缺Key时连接不可启动且离线控制隐藏") {
            let frame = try await session.wait(Expectation(anyOf: ["连接", "Connect"], timeout: 5))
            guard let connect = frame.exact("连接") ?? frame.exact("Connect") else { throw E2EError.product("缺少连接按钮：\(frame.summary)") }
            try await session.click(local: connect.center, on: frame)
            let still = try await session.wait(Expectation(anyOf: ["需要 API Key", "API Key required"], absent: ["正在连接", "Connecting", "通话中", "In call"], timeout: 3))
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.gpt_live.credential_ref") == nil else { throw E2EError.product("空设置不应产生凭据引用") }
            try session.save(still, "02-live-disabled.png")
            return "点击缺Key连接按钮后仍未连接，无凭据引用；离线静音/停止播报/任务核对默认隐藏"
        }
        try await session.step("选择音色并核对持久化") {
            // 滚动惯性会让落点逐次不同：按步滚动直到两个音色都出现，而不是依赖固定距离。
            for _ in 0..<8 {
                let frame = try await session.capture()
                if frame.contains("Marin") && frame.contains("Cedar") { break }
                try await session.scroll(pixels: -230)
                try await Task.sleep(nanoseconds: 400_000_000)
            }
            _ = try await session.wait(Expectation(allOf: ["Marin", "Cedar"], timeout: 5))
            try await session.click(matching: ["Cedar"], pick: .top)
            try await waitValue(session, "voice.gpt_live.voice", "cedar")
            let audio = try await session.wait(Expectation(allOf: ["Cedar", "Marin"], timeout: 5))
            try session.save(audio, "03-audio-dark.png")
            return "Cedar由真实按钮保存，SQLite=cedar"
        }
        try await session.step("GPT配置简化且交互方式独立保存") {
            // 上一步已把「对话偏好」滚进视图，交互方式与音色同在一张卡片，不再盲滚。
            // 用 wait 返回的同一帧定位，避免滚动惯性让第二次截图的文字位置变化。
            let modes = try await session.wait(Expectation(anyOf: ["按住说话", "Push to talk"], timeout: 5))
            guard let hold = modes.pick(["按住说话", "Push to talk"], .top) else {
                throw E2EError.product("未识别到「按住说话」按钮。可见：\(modes.summary)")
            }
            try await session.click(local: hold.center, on: modes)
            try await waitValue(session, "voice.gpt_live.interaction_mode", "single_voice_command")
            let simple = try await session.wait(Expectation(anyOf: ["Marin", "Cedar"], absent: ["智能体供应商", "Agent Provider", "Ollama", "OpenAI 兼容", "OpenAI Compatible", "准备语音检测", "Prepare speech detection", "项目工具", "Project tools"], timeout: 5))
            try session.save(simple, "03b-simple-dark.png")
            return "按住模式独立保存；GPT页面底部无供应商、模型和手工VAD配置，未启动云端通话"
        }
        try await session.step("诊断按需展开并可收起") {
            _ = try await session.wait(Expectation(anyOf: ["诊断", "Diagnostics"], absent: ["刷新核对", "Refresh reconciliation"], timeout: 5))
            try await session.click(matching: ["诊断", "Diagnostics"], pick: .bottom)
            try await session.scroll(pixels: -300)
            _ = try await session.wait(Expectation(anyOf: ["刷新核对", "Refresh reconciliation"], timeout: 5))
            try await session.click(matching: ["刷新核对", "Refresh reconciliation"], pick: .bottom)
            let report = try await session.wait(Expectation(anyOf: ["请先打开项目", "Open a project"], timeout: 5))
            try session.save(report, "03c-diagnostics-dark.png")
            try await session.click(matching: ["收起诊断", "Hide diagnostics"], pick: .bottom)
            _ = try await session.wait(Expectation(absent: ["刷新核对", "Refresh reconciliation"], timeout: 5))
            return "显式展开才展示诊断，真实核对报告无项目；收起不删除核对能力"
        }
        try await session.step("保存测试Key后默认收起，可更换和移除") {
            try await session.scroll(pixels: 1000)
            _ = try await session.wait(Expectation(anyOf: ["未配置", "Not configured"], timeout: 5))
            try await session.click(matching: ["未配置", "Not configured"], pick: .top)
            try await session.type("f")
            _ = try await session.wait(Expectation(anyOf: ["已安全保存", "己安全保存", "Saved securely"], allOf: ["API Key"], timeout: 8))
            try await session.click(matching: ["移除", "Remove"], pick: .top)
            _ = try await session.wait(Expectation(anyOf: ["未配置", "Not configured"], timeout: 8))
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.gpt_live.credential_ref") == nil else { throw E2EError.product("移除后仍有凭据引用") }
            try await session.click(matching: ["未配置", "Not configured"], pick: .top)
            try await session.shortcut(key: 36, modifiers: [])
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.gpt_live.credential_ref") == nil else { throw E2EError.product("移除后按Enter重新保存了旧Key") }
            try await session.click(matching: ["未配置", "Not configured"], pick: .top)
            try await session.type("f")
            _ = try await session.wait(Expectation(anyOf: ["已安全保存", "己安全保存", "Saved securely"], allOf: ["API Key"], timeout: 8))
            try await session.type("ixture-only-live-layout-key")
            _ = try await session.wait(Expectation(anyOf: ["已安全保存", "己安全保存", "Saved securely"], allOf: ["API Key"], timeout: 8))
            try await session.click(matching: ["隐藏", "Hide"], pick: .top)
            _ = try await session.wait(Expectation(anyOf: ["更换", "更換", "Change"], absent: ["移除", "Remove"], timeout: 5))
            let saved = try await session.wait(Expectation(anyOf: ["已安全保存", "己安全保存", "Saved securely"], allOf: ["API Key"], absent: ["fixture-only-live-layout-key"], timeout: 8))
            guard let reference = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.gpt_live.credential_ref"), reference.hasPrefix("gpt-live-desktop-") else { throw E2EError.product("测试凭据没有持久化") }
            guard try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "credential.agent-provider.\(reference)") == "fixture-only-live-layout-key" else { throw E2EError.product("停顿后续写未保存完整的本地无效夹具；不输出凭据值") }
            try session.save(saved, "03d-key-saved-dark.png")
            try await session.click(matching: ["更换", "更換", "Change"], pick: .top)
            let editing = try await session.wait(Expectation(anyOf: ["移除", "Remove"], absent: ["fixture-only-live-layout-key"], timeout: 5))
            try session.save(editing, "03e-key-edit-dark.png")
            try await session.click(matching: ["隐藏", "Hide"], pick: .top)
            _ = try await session.wait(Expectation(absent: ["移除", "Remove"], timeout: 5))
            return "仅使用本地无效夹具；活动编辑期间移除后Enter不复活旧Key；逐字续写后可直接鼠标隐藏，再由更换入口展开和收起"
        }
        try await session.step("浅色主题重启后恢复模式与音色") {
            session.stop()
            try VoiceConfigurationFixture.theme(database: session.databaseURL(), id: "paper-blue")
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await openVoice(session)
            let live = try await session.wait(Expectation(anyOf: ["已安全保存", "己安全保存", "Saved securely"], allOf: ["GPT-Live"], absent: ["刷新核对", "Refresh reconciliation", "fixture-only-live-layout-key"], timeout: 8))
            try session.save(live, "04-live-light.png")
            try await session.scroll(pixels: -230)
            let audio = try await session.wait(Expectation(allOf: ["Marin", "Cedar"], timeout: 5))
            try session.save(audio, "05-audio-light.png")
            try await waitValue(session, "voice.desktop.mode", "gpt_live")
            try await waitValue(session, "voice.gpt_live.voice", "cedar")
            try await waitValue(session, "voice.gpt_live.interaction_mode", "single_voice_command")
            return "重启后仍为GPT-Live/Cedar/按住说话，Key收起，浅色主题；选中状态需看图"
        }
        try await session.step("切回豆包与语音指令，GPT配置保留") {
            try await session.scroll(pixels: 1000)
            _ = try await session.wait(Expectation(anyOf: ["对话模式", "Conversation mode"], timeout: 5))
            try await session.click(matching: ["豆包", "Doubao 2.0 Voice"], pick: .top)
            _ = try await session.wait(Expectation(anyOf: ["豆包连接", "Doubao Connection"], timeout: 5))
            try await waitValue(session, "voice.desktop.mode", "doubao")
            try session.save(try await session.capture(), "06-doubao-regression.png")
            try await session.click(matching: ["语音指令", "Voice Commands"], pick: .top)
            try await waitValue(session, "voice.desktop.mode", "legacy")
            try await waitValue(session, "voice.gpt_live.voice", "cedar")
            try session.save(try await session.capture(), "07-legacy-regression.png")
            return "UI切换Legacy/Doubao均落库，GPT音色保留；无真实通话测试"
        }
    }

    private static func openVoice(_ session: Session) async throws {
        try await session.shortcut(key: 43, modifiers: [0x37])
        _ = try await session.wait(Expectation(anyOf: ["个性", "Persona", "人格", "语音", "Voice"], timeout: 8))
        try await session.click(matching: ["语音", "Voice"], pick: .left)
        _ = try await session.wait(Expectation(anyOf: ["对话模式", "Conversation mode"], timeout: 8))
    }

    private static func waitValue(_ session: Session, _ key: String, _ expected: String) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if try VoiceConfigurationFixture.value(database: session.databaseURL(), key: key) == expected { return }
            try await Task.sleep(nanoseconds: 120_000_000)
        }
        throw E2EError.product("设置\(key)未保存为\(expected)")
    }
}
