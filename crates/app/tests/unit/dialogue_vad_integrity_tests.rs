use super::{resolve_asr_recognizer_factory, DialogueStartError};
use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};
use voice_sherpa::SpeechError;

/// VAD-START-1：语音启动前必须重新校验共享 VAD，损坏文件不得进入 Provider 或麦克风阶段
///
/// 数据构造（含关键数值的推导过程）：
///   官方 VAD 大小 = 643,854 bytes（来自内置目录常量）
///   损坏 VAD 大小 = 1 byte（写入单字节 0x01）
///   大小差 = 643,854 - 1 = 643,853 bytes，因此 SHA-256 必定与目录值不一致
///   Azure 配置项 = 4（region + locale + credential_ref + ready=true）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建符合目录结构但内容仅 1 byte 的 silero_vad.onnx
///   2. 构造已验证并激活的 Azure ASR 设置
///   3. 调用语音启动阶段的 RecognizerFactory 解析入口
///   4. 入口先校验 VAD SHA-256，再决定是否解析 Provider 凭据
///
/// 预期结果：
///   - 正断言：返回 SpeechError::ModelNotFound，错误中明确包含共享支持文件 SHA 不匹配
///   - 负断言：不得返回 Provider 错误，证明损坏 VAD 在访问 Azure 凭据前已被拦截
#[test]
fn corrupt_shared_vad_is_rejected_before_provider_or_microphone_start() {
    let model_root = tempfile::tempdir().expect("测试应创建隔离的模型根目录");
    let vad_root = model_root.path().join(asr::SILERO_VAD_DIRECTORY);
    std::fs::create_dir_all(&vad_root).expect("测试应创建 VAD 模型目录");
    std::fs::write(vad_root.join(asr::SILERO_VAD_FILE), [0x01_u8])
        .expect("测试应写入 1 byte 的损坏 VAD 文件");
    let state = VoiceSettingsState::from_sources_with_model_root(
        SavedVoiceSettings {
            asr_provider: Some(asr::AsrProviderId::CloudAzure),
            azure_asr_region: Some("eastus".to_string()),
            azure_asr_locale: Some("zh-CN".to_string()),
            azure_asr_credential_ref: Some("azure-vad-integrity-test".to_string()),
            azure_asr_ready: Some(true),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
        model_root.path().to_path_buf(),
    );

    let result = resolve_asr_recognizer_factory(state.values(), model_root.path());

    assert!(
        matches!(
            &result,
            Err(DialogueStartError::Speech(SpeechError::ModelNotFound(message)))
                if message.contains(
                    "shared support artifact SHA-256 does not match the catalog"
                )
        ),
        "损坏 VAD 必须以完整性错误阻止语音启动"
    );
    assert!(
        !matches!(result, Err(DialogueStartError::Provider(_))),
        "VAD 完整性失败必须发生在 Provider 凭据解析之前"
    );
}
