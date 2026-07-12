use super::{DialogueControlState, DialogueStartError};
use crate::app::voice::control::VoiceVisual;
use crate::settings::voice::{
    DialogueLlmMode, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsValues,
};
use dialogue::DialoguePhase;
use tempfile::tempdir;
use voice_sherpa::SpeechError;

fn mic_ok() -> Result<(), SpeechError> {
    Ok(())
}

/// DLG-11: Dialogue LLM 默认应为 Mock，便于无 Ollama 时跑通测试与离线开发。
///
/// 数据构造（含关键数值的推导过程）：
///   saved/env 均未设置 dialogue_llm_mode
///
/// 执行过程（逐步说明系统如何处理）：
///   1. VoiceSettingsValues::resolve 使用默认 saved/env
///
/// 预期结果：
///   - 正断言：dialogue_llm_mode() == Mock
///   - 负断言：默认不应要求 Ollama
#[test]
fn test_dialogue_llm_mode_defaults_to_mock() {
    let values =
        VoiceSettingsValues::resolve(&SavedVoiceSettings::default(), &EnvVoiceSettings::default());

    assert_eq!(
        values.dialogue_llm_mode(),
        DialogueLlmMode::Mock,
        "未配置时 Dialogue LLM 应默认为 Mock"
    );
}

/// DLG-12: Ollama 模式下 Continuous Listening 启动前应探测 readiness，失败时不进入 Listening。
///
/// 数据构造（含关键数值的推导过程）：
///   dialogue_llm_mode = Ollama
///   ollama endpoint = http://127.0.0.1:1（无服务监听）
///   model_dir = tempfile 现存目录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. begin_listening_sync_for_tests 校验 mic/provider/ollama
///   2. probe_ollama_ready 连接失败
///   3. 设置 error_message 并保持 Idle
///
/// 预期结果：
///   - 正断言：返回 Err(DialogueStartError::Ollama(...))
///   - 正断言：phase 仍为 Idle，error_message 含 Ollama unavailable
///   - 负断言：不应进入 Listening
#[test]
fn test_dialogue_start_blocks_listening_when_ollama_unavailable() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let _installed = super::app_dialogue_shell_tests::continuous_listening_values(Some(
            dir.path().to_path_buf(),
        ));
        let saved = SavedVoiceSettings {
            interaction_mode: Some(
                crate::settings::voice::VoiceInteractionMode::ContinuousListening,
            ),
            asr_model_id: Some(
                asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID)
                    .expect("内置 Paraformer ID 应有效"),
            ),
            dialogue_llm_mode: Some(DialogueLlmMode::Ollama),
            ollama_endpoint: Some("http://127.0.0.1:1".to_string()),
            ..SavedVoiceSettings::default()
        };
        let values = VoiceSettingsValues::resolve(&saved, &EnvVoiceSettings::default());
        let mut state = DialogueControlState::default();

        DialogueControlState::set_mic_permission_check_for_tests(Some(mic_ok));
        let error = state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect_err("Ollama 不可达时启动应失败");

        assert!(
            matches!(error, DialogueStartError::Ollama(_)),
            "启动失败应归类为 DialogueStartError::Ollama，实际: {error:?}"
        );
        assert_eq!(
            state.phase(),
            DialoguePhase::Idle,
            "Ollama 探测失败时不应进入 Listening"
        );
        assert_eq!(
            state.dialogue_voice_visual(),
            Some(VoiceVisual::Error),
            "失败时底栏应显示 Error 视觉态"
        );
        assert!(
            state
                .dialogue_voice_label()
                .is_some_and(|label| label.contains("Ollama unavailable")),
            "底栏应展示 Ollama unavailable 错误文案"
        );
    });
}
