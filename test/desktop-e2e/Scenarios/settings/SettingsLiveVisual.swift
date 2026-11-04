import E2EKit
import Fixtures
import Foundation

/// GPT-Live 设置按 docs/prototypes/gpt-live-settings 重构后的模型看图验收；不调用系统 Vision OCR，不连接云端。
@MainActor
enum SettingsLiveVisual {
    static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("深色：选择GPT-Live，单栏配置页") {
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1280, height: 860)
            _ = try await session.visualCheck([VisualCriterion(id: "boot", text: "Norma 主界面已显示（工作空间起始页），不是启动动画或空白窗口；若仍在加载请回答 wait")])
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("设置侧栏中的「语音」入口")
            try await session.visualClick("「对话模式」三张卡片中标题为「GPT-Live」的卡片中心")
            try await SettingsLiveConfiguration.waitValue(session, "voice.desktop.mode", "gpt_live")
            let page = try await session.visualCheck([
                VisualCriterion(id: "modes", text: "对话模式仍是三张卡片：语音指令、豆包 2.0 语音、GPT-Live；GPT-Live 卡片为选中态（强调色描边与勾）"),
                VisualCriterion(id: "heading", text: "卡片下方有方形图标底 + 「GPT-Live」标题 + 一行说明，之后是带钥匙图标的「OpenAI API Key」圆角卡片，右侧橙色「需要 API Key」，下方有空的密钥输入框"),
                VisualCriterion(id: "no-call", text: "页面上没有「连接」「挂断」「静音」「停止播报」按钮，也没有独立的连接状态卡片"),
                VisualCriterion(id: "prefs", text: "可见「对话偏好」小标题，右侧有「更改会自动保存」说明（如被视口截断可只看到小标题）")
            ])
            try session.save(page, "01-live-dark.png")
            return "GPT-Live已选（SQLite=gpt_live），单栏配置无通话控制已看图"
        }
        try await session.step("深色：偏好行分段控件、音色单选与麦克风") {
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -260)
            try await session.visualClick("「交互方式」行右侧分段控件中的「按住说话」一段")
            try await SettingsLiveConfiguration.waitValue(session, "voice.gpt_live.interaction_mode", "single_voice_command")
            try await session.visualClick("「回复音色」行右侧的「Cedar」单选项")
            try await SettingsLiveConfiguration.waitValue(session, "voice.gpt_live.voice", "cedar")
            let prefs = try await session.visualCheck([
                VisualCriterion(id: "card", text: "「对话偏好」为一张圆角描边卡片，三行之间有细分隔线：交互方式、回复音色、麦克风"),
                VisualCriterion(id: "segmented", text: "交互方式右侧是一个胶囊形分段控件，「按住说话」一段为强调色浅底并带勾，「实时对话」未选中；说明文字改为按住快捷键说话"),
                VisualCriterion(id: "radio", text: "回复音色为 Marin、Cedar 两个描边选项，Cedar 带实心圆点并高亮，Marin 为空心圆"),
                VisualCriterion(id: "mic", text: "麦克风行左侧为麦克风图标、标题与带彩色小圆点的状态文字，右侧有「声音设置」文字按钮（带外链图标）和刷新图标按钮")
            ])
            try session.save(prefs, "02-prefs-dark.png")
            return "按住说话与Cedar经真实点击落库；分段/单选/麦克风行已看图"
        }
        try await session.step("深色：诊断默认收起，展开后显示连接详情") {
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -1200)
            _ = try await session.visualCheck([VisualCriterion(id: "collapsed", text: "底部有细线分隔的「诊断与连接详情」整行（左侧滑杆图标，右侧「仅在排查问题时需要」与向下箭头），处于收起状态；最底部有「所有更改已保存」和「麦克风跟随 macOS 系统默认输入」页脚")])
            try await session.visualClick("「诊断与连接详情」整行")
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -1200)
            let diagnostics = try await session.visualCheck([
                VisualCriterion(id: "facts", text: "展开后显示语音服务 OpenAI Realtime、输入设备、连接方式 WebSocket、音频缓冲区上限 200 毫秒"),
                VisualCriterion(id: "raw", text: "有浅底等宽文字块显示「当前连接没有错误。」，并有「刷新核对」按钮与「诊断信息不包含 API Key。」说明；箭头变为向上")
            ])
            try session.save(diagnostics, "03-diagnostics-dark.png")
            try await session.visualClick("「诊断与连接详情」行最右侧向上的箭头按钮中心")
            _ = try await session.visualCheck([VisualCriterion(id: "chevron-collapsed", text: "点击箭头后诊断只切换一次而保持收起：箭头向下，WebSocket、原始报错块与「刷新核对」均不可见")])
            try await session.visualClick("「诊断与连接详情」行最右侧向下的箭头按钮中心")
            _ = try await session.visualCheck([VisualCriterion(id: "chevron-expanded", text: "再次点击箭头后诊断只切换一次而保持展开：箭头向上，WebSocket、原始报错块与「刷新核对」重新出现；若内容被底部裁切，标题右侧向上箭头必须可见")])
            return "整行与箭头均可切换，箭头收起/展开各一次，事实与原始报错区已看图"
        }
        try await session.step("深色：Key保存、更换自动焦点、失焦收起与移除") {
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: 1600)
            try await session.visualClick("OpenAI API Key 卡片内显示「未配置」的空密钥输入框中心")
            try await session.type("f")
            try await waitFixtureKey(session, expected: "f")
            _ = try await session.visualCheck([VisualCriterion(id: "key-editing", text: "首字符已保存后 API Key 的密钥输入框仍保持展开，显示掩码字符而非明文；右侧有「完成」，下方有「移除」")])
            try await session.type("ixture-only-live-layout-key")
            try await waitFixtureKey(session, expected: "fixture-only-live-layout-key")
            try await session.visualClick("OpenAI API Key 卡片右侧的「完成」按钮")
            let saved = try await session.visualCheck([VisualCriterion(id: "key-saved", text: "API Key 行显示「已安全保存」与「更换」；密钥输入框、「完成」与「移除」均已收起，且没有本地测试Key的明文")])
            try session.save(saved, "03d-key-saved-dark.png")
            try await session.visualClick("OpenAI API Key 卡片右侧的「更换」按钮")
            // Do not click the editor: typing must follow the focus established by Change.
            try await session.type("fixture-only-live-focus-key")
            try await waitFixtureKey(session, expected: "fixture-only-live-focus-key")
            try await session.shortcut(key: 48, modifiers: [])
            let blurred = try await session.visualCheck([VisualCriterion(id: "key-blurred", text: "按Tab令Key输入框失焦后，API Key行保持「已安全保存」与「更换」，输入框、「完成」与「移除」均已收起，无密钥明文")])
            try session.save(blurred, "03e-key-blur-dark.png")
            try await session.visualClick("OpenAI API Key 卡片右侧的「更换」按钮")
            try await session.visualClick("API Key 编辑区内的「移除」按钮")
            try await waitFixtureKey(session, expected: nil)
            try await session.visualClick("OpenAI API Key 卡片内显示「未配置」的空密钥输入框中心")
            try await session.shortcut(key: 36, modifiers: [])
            let removed = try await session.visualCheck([VisualCriterion(id: "key-removed", text: "移除后在空编辑器按Enter，API Key仍显示「需要 API Key」，输入框为空并显示「未配置」，没有「已安全保存」或旧Key掩码")])
            try await waitFixtureKey(session, expected: nil)
            try session.save(removed, "03f-key-removed-dark.png")
            return "仅本地无效Key；首字符后续写完整，更换后直接输入成功，Tab失焦折叠，移除后Enter不复活"
        }
        try await session.step("英文蓝宝石之夜主题：重启后恢复偏好与双语文案") {
            session.stop()
            try VoiceConfigurationFixture.theme(database: session.databaseURL(), id: "sapphire-night")
            try session.launch(language: "en")
            try await session.resize(width: 1280, height: 860)
            _ = try await session.visualCheck([VisualCriterion(id: "boot", text: "Norma 主界面已显示，工作空间起始页使用英文（如 Create Workspace）；若仍在加载请回答 wait")])
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("Settings 侧栏中标为 Voice 的语音入口")
            _ = try await session.visualCheck([VisualCriterion(id: "english-key", text: "GPT-Live 页面上 OpenAI API Key 卡片显示英文 API Key required 与 Not configured；下方为 Conversation preferences 和 Changes save automatically，没有「需要 API Key」「未配置」「对话偏好」等中文固定文案")])
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -260)
            let light = try await session.visualCheck([
                VisualCriterion(id: "theme", text: "Sapphire Night 主题下英文 GPT-Live 页的卡片描边、分隔线、分段控件与单选选中态仍清晰可辨，无错位"),
                VisualCriterion(id: "restored", text: "GPT-Live 仍选中；分段控件的 Push to talk 与音色 Cedar 仍为选中态；另一个分段显示 Real-time conversation"),
                VisualCriterion(id: "english-prefs", text: "偏好行固定文案显示 Interaction mode、Reply voice、Microphone、Sound settings，没有「交互方式」「回复音色」「麦克风」「声音设置」等中文固定文案；系统返回的设备名称可保留原文")
            ])
            try session.save(light, "04-live-sapphire.png")
            try await SettingsLiveConfiguration.waitValue(session, "voice.gpt_live.voice", "cedar")
            try await SettingsLiveConfiguration.waitValue(session, "voice.gpt_live.interaction_mode", "single_voice_command")
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -1200)
            let footer = try await session.visualCheck([
                VisualCriterion(id: "english-diagnostics", text: "诊断默认收起，标题为 Diagnostics & connection details，右侧为 For troubleshooting only；没有「诊断与连接详情」「仅在排查问题时需要」等中文固定文案"),
                VisualCriterion(id: "english-saved", text: "页脚以英文显示 All changes saved 和 Uses the default macOS microphone，没有「所有更改已保存」或「麦克风跟随」等中文固定文案，也不同时显示 Saving... 或保存失败提示")
            ])
            try session.save(footer, "04b-footer-sapphire-en.png")
            return "英文Sapphire Night重启后偏好保留，Key/偏好/诊断/保存状态文案无中英混排，已看图（产品已无浅色主题）"
        }
    }

    /// Inspect only the isolated fixture store; never print the key in an assertion or step result.
    private static func waitFixtureKey(_ session: Session, expected: String?) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            let reference = try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "voice.gpt_live.credential_ref")
            if let expected {
                if let reference, reference.hasPrefix("gpt-live-desktop-"),
                   try VoiceConfigurationFixture.value(database: session.databaseURL(), key: "credential.agent-provider.\(reference)") == expected { return }
            } else if reference == nil { return }
            try await Task.sleep(nanoseconds: 120_000_000)
        }
        throw E2EError.product(expected == nil ? "移除后仍有GPT-Live凭据引用" : "本地测试Key未完整持久化；不输出凭据值")
    }
}
