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

/// MiniMax 临时网络故障：已完成的 Voice 配置必须保留凭据与配置就绪状态。
///
/// 数据构造（含关键数值的推导过程）：
///   Voice 必需项 = 4（麦克风 + ASR + Agent Provider + Spoken Replies）
///   初始 Ready 项 = 4
///   临时故障数 = 1（MiniMax Transport）
///   持久化凭据 = 1 组（credential ref + 内存中的 Keychain secret）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造已经完成 setup 的 MiniMax 配置，并把麦克风、ASR 与 Agent Provider 标记 Ready。
///   2. 用已保存的 API Key 完成 MiniMax 连接与手动 Voice 验证，使四项均 Ready。
///   3. 再次连接时注入 Transport 故障，模拟离线、DNS 或服务端暂时不可达。
///   4. 读取 MiniMax 账户状态、credential ref、secret 与 Voice 总体 readiness。
///
/// 预期结果：
///   - 正断言：MiniMax 细分状态为 TemporarilyUnavailable，供界面提示重试。
///   - 正断言：credential ref 与 secret 都继续存在，配置仍视为 Ready。
///   - 负断言：临时网络故障不得把 Voice 送回 Spoken Replies 修复/重新配置流程。
#[test]
fn temporary_minimax_network_failure_preserves_configured_voice_state() {
    let minimax_provider = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
        .expect("内置 MiniMax Provider ID 必须有效");
    let minimax_model = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("内置 MiniMax Turbo model ID 必须有效");
    let minimax_voice =
        tts::TtsVoiceId::parse("manual_voice_network_test").expect("测试 Voice ID 必须有效");
    let credential_ref = "keychain:minimax:network-test";
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            asr_model_id: asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID).ok(),
            active_tts_provider_id: Some(minimax_provider),
            minimax_tts_model_id: Some(minimax_model),
            minimax_tts_voice_id: Some(minimax_voice),
            minimax_tts_voice_provenance: Some(tts::VoiceSelectionProvenance::Manual),
            minimax_tts_credential_ref: Some(credential_ref.to_string()),
            minimax_tts_api_key: Some("preserved-network-test-key".to_string()),
            setup_completed: Some(true),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    state.set_microphone_requirement(VoiceRequirementState::Ready);
    state.set_provider_requirement(VoiceRequirementState::Ready);
    let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("内置 Zipformer Bilingual model ID 必须有效");
    state.model_manager_mut().mark_ready(&asr_id);

    let (initial_generation, initial_key) = state
        .begin_minimax_connection()
        .expect("已保存 MiniMax API Key 必须能发起初始连接");
    assert_eq!(
        initial_key, "preserved-network-test-key",
        "初始连接必须使用 Keychain 已加载的 API Key"
    );
    assert!(
        state.record_minimax_connection_success(
            initial_generation,
            initial_key,
            tts::ProviderVoiceCatalog::default(),
        ),
        "当前 generation 的初始 MiniMax 连接结果必须被接纳"
    );
    assert!(
        state.mark_minimax_manual_voice_validated(state.minimax_profile_generation()),
        "手动 Voice 必须先完成验证，使网络故障前的配置确实 Ready"
    );
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "网络故障前四项已配置能力必须让 Voice 为 Ready"
    );

    let (failure_generation, failure_key) = state
        .begin_minimax_connection()
        .expect("已保存 MiniMax API Key 必须允许重试连接");
    assert_eq!(
        state.record_minimax_connection_failure(
            failure_generation,
            tts::TtsErrorKind::Transport,
            "simulated temporary network outage",
        ),
        super::super::MiniMaxConnectionFailureDisposition::ActiveCredentialFailed,
        "当前活动凭据的 Transport 故障必须记录为可恢复连接失败"
    );

    assert_eq!(
        failure_key, "preserved-network-test-key",
        "网络重试必须继续使用原 Keychain API Key"
    );
    assert_eq!(
        state.minimax_readiness(),
        tts::RemoteTtsReadiness::TemporarilyUnavailable,
        "Transport 故障必须与 InvalidCredential 明确区分"
    );
    assert_eq!(
        state.values().minimax_tts_credential_ref(),
        Some(credential_ref),
        "临时网络故障不得删除持久化 credential ref"
    );
    assert_eq!(
        state.values().minimax_tts_api_key(),
        Some("preserved-network-test-key"),
        "临时网络故障不得清空已加载的 Keychain secret"
    );
    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "临时网络故障只能提示重试，不得把已配置 Voice 送回重新配置流程"
    );
    assert_ne!(
        state.readiness(),
        VoiceEntryState::Unavailable {
            first_incomplete: VoiceRequirement::SpokenReplies,
        },
        "临时网络故障不得伪装成 Spoken Replies 配置失效"
    );
}
