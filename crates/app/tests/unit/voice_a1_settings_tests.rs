use super::*;
use crate::settings::voice::{
    DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode,
};

/// 场景：A1 只可显式开启本地单次识别；构造三种模式的相同已校验状态。
/// 执行 activate；预期云对话拒绝、默认/云配置未改变，本地只改变 ASR provider。
#[test]
fn a1_activation_does_not_change_desktop_or_cloud_modes() {
    for mode in [
        DesktopVoiceMode::Legacy,
        DesktopVoiceMode::Doubao,
        DesktopVoiceMode::GptLive,
    ] {
        let saved = SavedVoiceSettings {
            desktop_mode: Some(mode),
            ..SavedVoiceSettings::default()
        };
        let mut state = VoiceSettingsState::from_sources(saved, EnvVoiceSettings::default());
        assert_eq!(
            state.values().asr_provider(),
            asr::AsrProviderId::LocalSherpa,
            "升级默认必须保持 Sherpa"
        );
        assert!(!state.activate_local_a1(), "未校验不得激活");
        let generation = state.begin_a1_validation();
        assert!(
            state.finish_a1_validation(generation, Ok(())),
            "当前文件校验应生效"
        );
        assert_eq!(
            state.activate_local_a1(),
            mode == DesktopVoiceMode::Legacy,
            "仅本地模式允许激活"
        );
        assert_eq!(state.values().desktop_mode(), mode, "A1 不得改桌面对话模式");
        assert_eq!(
            state.values().interaction_mode(),
            VoiceInteractionMode::SingleVoiceCommand,
            "A1 不改快捷键交互模式"
        );
        if mode != DesktopVoiceMode::Legacy {
            assert_eq!(
                state.values().asr_provider(),
                asr::AsrProviderId::LocalSherpa,
                "云对话必须保留原本地配置"
            );
        }
    }
}

/// 场景：另一窗口在 A1 准备期间选择 Azure；构造同库新快照。
/// 执行快照同步，再提交旧准备结果；预期旧 token 取消、结果无效，不覆盖新选择。
#[test]
fn settings_sync_invalidates_pending_a1_activation() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    let generation = state.begin_a1_validation();
    let token = state.a1_preparation_cancellation();
    let mut latest = state.saved_snapshot();
    latest.asr_provider = Some(asr::AsrProviderId::CloudAzure);
    assert!(
        state.apply_saved_snapshot(latest, false),
        "应接受其他窗口的新设置"
    );
    assert!(token.is_cancelled(), "旧准备任务必须取消");
    assert!(
        !state.finish_a1_validation(generation, Ok(())),
        "迟到准备结果不得生效"
    );
    assert!(!state.activate_local_a1(), "不得用旧校验激活 A1");
    assert_eq!(
        state.values().asr_provider(),
        asr::AsrProviderId::CloudAzure,
        "保留新 Azure 选择"
    );
}

/// 场景：曾选择 A1 但本地模型不可用时，仍可使用已配置的豆包或 GPT-Live。
/// 构造隔离 SQLite 中的 A1 选择和两个假凭据引用，不连接服务、不读取真实配置。
/// 执行各云模式 readiness；预期云入口可用，A1 仍未准备，不要求加载本地模型。
#[test]
fn missing_a1_models_do_not_block_cloud_voice_readiness() {
    use crate::settings::voice::{VoiceEntryState, VoiceRequirementState};
    for mode in [DesktopVoiceMode::Doubao, DesktopVoiceMode::GptLive] {
        let directory = tempfile::tempdir().expect("隔离设置目录");
        let store = storage::SettingsStore::new(directory.path().join("app.sqlite"));
        for (key, value) in [
            ("voice.desktop.mode", mode.as_str()),
            ("voice.asr.provider", "local_a1"),
            ("voice.doubao.credential_ref", "fixture-doubao"),
            ("voice.gpt_live.credential_ref", "fixture-live"),
        ] {
            store.set_string(key, value).expect("隔离设置应可写入");
        }
        let mut state = VoiceSettingsState::load_from_store(store);
        state.set_microphone_requirement(VoiceRequirementState::Ready);
        assert_eq!(
            state.values().asr_provider(),
            asr::AsrProviderId::LocalA1,
            "切云模式不丢弃用户保存的本地选择"
        );
        assert_eq!(
            state.a1_requirement(),
            VoiceRequirementState::Missing,
            "夹具不得准备 A1 模型"
        );
        assert_eq!(
            state.readiness(),
            VoiceEntryState::Ready,
            "{mode:?} 的就绪状态不能被缺失的 A1 模型阻断"
        );
        assert!(!state.activate_local_a1(), "云模式不得激活 A1");
    }
}
