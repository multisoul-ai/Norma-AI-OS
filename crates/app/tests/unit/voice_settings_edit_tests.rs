use super::*;
use gpui::{Keystroke, Modifiers};

fn key(key: &str, key_char: Option<&str>) -> Keystroke {
    Keystroke {
        modifiers: Modifiers::default(),
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}
/// VS-8B：Voice Provider 文本字段必须在 KeyDown 路径直接接受 key_char，不能只等 IME。
///
/// 场景：用户单击 Base URL 进入编辑后直接敲键盘输入新主机名。
/// 数据构造：默认 OpenAI base URL 为 `https://api.openai.com/v1`；首个可打印键应整段替换，
/// 随后 `a` 追加；空格键写入一个空格。
/// 执行过程：begin_edit(AgentBaseUrl) → handle_key_down("h") → handle_key_down("a") →
/// handle_key_down("space")；另用 handle_text_input 验证 IME/粘贴路径仍可用。
/// 预期结果：
///   - 正断言：KeyDown 消费可打印键并得到 draft=`ha `，行文本带光标
///   - 正断言：IME handle_text_input 仍可追加
///   - 负断言：首个 KeyDown 不得把字符追加到旧 URL 末尾
#[test]
fn test_voice_provider_field_accepts_keydown_text_and_replaces_draft() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            agent_provider: Some(AgentProviderKind::OpenAiCompatible),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    state.begin_edit(VoiceSettingsField::BaseUrl);
    let old = state.draft.clone();

    assert!(
        state.handle_key_down(&key("h", Some("h"))),
        "可打印 KeyDown 应被 Voice 编辑态消费，避免落到画布/终端路由"
    );
    assert_eq!(
        state.draft, "h",
        "首次可打印输入应整段替换旧 draft，而不是追加"
    );
    assert_ne!(
        state.draft,
        format!("{old}h"),
        "不得把首字符追加到旧 Base URL 末尾"
    );

    assert!(
        state.handle_key_down(&key("a", Some("a"))),
        "后续可打印 KeyDown 应继续被消费"
    );
    assert!(
        state.handle_key_down(&key("space", None)),
        "空格 KeyDown 应写入草稿并被消费"
    );
    assert_eq!(state.draft, "ha ", "KeyDown 路径应得到 ha + 空格");
    assert_eq!(
        state.row_value(VoiceSettingsField::BaseUrl),
        "ha |",
        "编辑行应显示 KeyDown 草稿和光标"
    );

    assert!(
        state.handle_text_input("ime"),
        "IME/粘贴路径应在编辑态继续可用"
    );
    assert_eq!(
        state.draft, "ha ime",
        "handle_text_input 应追加到 KeyDown 已写入的草稿"
    );

    assert!(
        state.handle_key_down(&key("x", None)),
        "仅有 key、无 key_char 时也应写入（对齐 browser 地址栏回退）"
    );
    assert_eq!(
        state.draft, "ha imex",
        "key 回退路径应追加到已有草稿"
    );
}
