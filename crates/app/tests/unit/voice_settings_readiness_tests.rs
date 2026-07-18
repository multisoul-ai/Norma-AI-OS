use super::super::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};
use crate::settings::voice::{VoiceEntryState, VoiceRequirement, VoiceRequirementState};

/// VoiceSettingsState 汇总四项真实配置：仅当麦克风、已安装 ASR、已验证 Provider、已安装 TTS 音色都可用时才 Ready
///
/// 数据构造（含关键数值的推导过程）：
///   必需项总数 = 4
///   初始 Ready 项数 = 0
///   最终 Ready 项数 = 4
///   推荐 ASR = Zipformer Bilingual
///   推荐 TTS = Matcha Chinese-English + Default Bilingual Voice
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从空 saved/env 创建 state，确认首先缺麦克风
///   2. 将麦克风和 Provider 标记 Ready
///   3. 将推荐 ASR / TTS 标记安装完成并选中默认音色
///   4. 重新计算 readiness
///
/// 预期结果：
///   - 正断言：初始状态为 SetUp(Microphone)，最终状态为 Ready
///   - 负断言：TTS 未配置前不得提前 Ready
#[test]
fn settings_state_requires_all_four_runtime_capabilities() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    assert_eq!(
        state.readiness(),
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::Microphone,
        },
        "fresh settings state must start setup at microphone permission"
    );

    state.set_microphone_requirement(VoiceRequirementState::Ready);
    state.set_provider_requirement(VoiceRequirementState::Ready);
    let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("built-in Zipformer model id should parse");
    state.model_manager_mut().mark_ready(&asr_id);
    state.select_asr_model(Some(asr_id));

    assert_ne!(
        state.readiness(),
        VoiceEntryState::Ready,
        "Voice must remain unavailable until required TTS model and voice are selected"
    );

    let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
        .expect("built-in Matcha model id should parse");
    let voice_id =
        tts::TtsVoiceId::parse("default").expect("built-in bilingual voice id should parse");
    state.tts_model_manager_mut().mark_ready(&tts_id);
    state.select_tts_model_and_voice(Some(tts_id), Some(voice_id));

    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "all four usable capabilities must make Voice ready"
    );
    assert_ne!(
        state.readiness(),
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::SpokenReplies,
        },
        "selected ready TTS must clear the Spoken Replies setup gate"
    );
}

/// 独立模型安装进度：尚未选中的 TTS 下载也必须让底部入口显示 SettingUp
///
/// 数据构造（含关键数值的推导过程）：
///   TTS total = Matcha catalog download_bytes
///   TTS completed = TTS total / 2
///   expected% = (TTS total / 2) * 100 / TTS total = 50%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 保持 ASR、TTS 都未选择
///   2. 从 Spoken Replies 独立启动 Matcha 下载并推进到一半
///   3. 从所有活跃模型任务计算入口进度
///
/// 预期结果：
///   - 正断言：入口显示 SettingUp 50%
///   - 负断言：不得因为模型尚未选中而退回 SetUp
#[test]
fn unselected_independent_tts_install_projects_bottom_entry_progress() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
        .expect("built-in Matcha model id should parse");
    let tts_catalog = tts::builtin_model_catalog();
    let tts_total = tts_catalog
        .find(&tts_id)
        .expect("built-in Matcha TTS must exist in catalog")
        .package()
        .download_bytes();
    // completed = tts_total / 2; active_total = tts_total; percent = 50%.
    state
        .tts_model_manager_mut()
        .begin_install(&tts_id, tts_total);
    state
        .tts_model_manager_mut()
        .update_progress(&tts_id, tts_total / 2, tts_total);

    assert_eq!(
        state.readiness(),
        VoiceEntryState::SettingUp { percent: 50 },
        "an active independent TTS download must project its exact progress into the bottom entry"
    );
    assert_ne!(
        state.readiness(),
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::Microphone,
        },
        "an unselected model download must not be hidden behind the generic setup entry"
    );
}
