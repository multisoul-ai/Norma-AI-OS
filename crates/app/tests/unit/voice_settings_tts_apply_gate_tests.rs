use super::*;

/// TTS-PICKER-5：ZipVoice 在参考 WAV 与准确转写都就绪之前不能应用。
///
/// 数据构造（含关键数值的推导过程）：
///   model = zipvoice-distill-int8-zh-en-emilia
///   voice = cloned_reference
///   WAV = 16 kHz × 3 s × 1 ch × 16 bit = 48,000 samples
///   transcript = 「这是准确的参考音频转写。」
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开选择器并选中已安装的 ZipVoice 克隆音色；
///   2. 在参考声音未就绪时读取 tts_picker_ready；
///   3. 导入合法 WAV 与转写后再读取。
///
/// 预期结果：
///   - 正断言：参考声音导入后选择器变为可应用；
///   - 负断言：只有模型、没有参考 WAV 与转写时不得可应用，当前生效音色不变。
#[test]
fn test_zipvoice_draft_stays_unready_until_reference_wav_and_transcript_exist() {
    let model = tts::TtsModelId::parse(tts::ZIPVOICE_DISTILL_INT8_MODEL_ID)
        .expect("ZipVoice model ID 必须有效");
    let voice = tts::TtsVoiceId::parse("cloned_reference").expect("克隆 Voice ID 必须有效");
    let active_model =
        tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID).expect("Kokoro model ID 必须有效");
    let active_voice = tts::TtsVoiceId::parse("af_bella").expect("Bella Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(active_model.clone()),
            tts_voice_id: Some(active_voice.clone()),
            active_tts_provider_id: Some(
                tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
                    .expect("On-device Provider ID 必须有效"),
            ),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    state.tts_model_manager_mut().mark_ready(&model);
    state.open_tts_picker();
    state.set_tts_picker_local_selection(model.clone(), voice.clone());

    assert!(
        !state.tts_picker_ready(),
        "未导入参考 WAV 与转写时 ZipVoice 草稿不得可应用"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&active_voice),
        "未应用的克隆草稿不得改变当前 Bella"
    );

    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let source = temp.path().join("speaker.wav");
    write_reference_wav(&source);
    let imported = tts::import_zipvoice_reference(
        &source,
        "这是准确的参考音频转写。",
        &temp.path().join("models"),
    )
    .expect("合法参考 WAV 与转写应导入成功");
    state.apply_zipvoice_reference_scan(Ok(imported));

    assert!(
        state.zipvoice_reference_ready(),
        "导入后参考声音必须标记为就绪"
    );
    assert!(
        state.tts_picker_ready(),
        "参考 WAV 与准确转写都就绪后 ZipVoice 草稿才可应用"
    );
    assert_ne!(
        state.values().tts_model_id(),
        Some(&model),
        "就绪只表示可以应用，不得提前替换当前 Kokoro"
    );
}

/// TTS-PICKER-6：手动 MiniMax Voice ID 在验证之前不能提交。
///
/// 数据构造（含关键数值的推导过程）：
///   account voices = 1（catalog_voice，不是用户输入的 manual_voice）
///   manual voice = manual_voice
///   validation proofs = 0，直到 mark_tts_picker_minimax_validated
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连接已保存凭据并打开 MiniMax 选择器；
///   2. 写入不在目录中的手动 Voice ID，尝试提交；
///   3. 标记该草稿已验证后再提交。
///
/// 预期结果：
///   - 正断言：验证后 readiness 为 Ready，提交保存 manual_voice；
///   - 负断言：未验证时不得 Ready，也不得写入已保存 Profile。
#[test]
fn test_manual_minimax_voice_id_cannot_apply_until_validated() {
    let turbo = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
        .expect("MiniMax Turbo model ID 必须有效");
    let catalog_voice = tts::TtsVoiceId::parse("catalog_voice").expect("目录 Voice ID 必须有效");
    let manual_voice = tts::TtsVoiceId::parse("manual_voice").expect("手动 Voice ID 必须有效");
    let saved_voice = tts::TtsVoiceId::parse("saved_voice").expect("已保存 Voice ID 必须有效");
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            minimax_tts_model_id: Some(turbo.clone()),
            minimax_tts_voice_id: Some(saved_voice.clone()),
            minimax_tts_voice_provenance: Some(tts::VoiceSelectionProvenance::Catalog),
            minimax_tts_credential_ref: Some("picker-test-account".to_string()),
            minimax_tts_api_key: Some("synthetic-picker-key".to_string()),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    let (generation, key) = state
        .begin_minimax_connection()
        .expect("预置测试凭据必须允许连接");
    assert!(
        state.record_minimax_connection_success(
            generation,
            key,
            tts::ProviderVoiceCatalog::new(vec![tts::ProviderVoiceDescriptor::new(
                catalog_voice,
                "Catalog Voice",
                tts::TtsVoiceOrigin::System,
            )]),
        ),
        "当前 MiniMax 连接结果必须被接纳"
    );

    state.open_tts_picker();
    state.select_tts_picker_provider(tts::MINIMAX_TTS_PROVIDER_ID);
    state.set_tts_picker_minimax_selection(
        turbo.clone(),
        manual_voice.clone(),
        tts::VoiceSelectionProvenance::Manual,
    );

    assert_eq!(
        state.tts_picker_minimax_readiness(),
        tts::RemoteTtsReadiness::VoiceValidationRequired,
        "不在目录中的手动 Voice ID 必须先验证"
    );
    assert!(
        !state.tts_picker_ready(),
        "未验证的 MiniMax Voice ID 不得使选择器可应用"
    );
    assert!(
        !state.commit_tts_picker_minimax_profile(),
        "未验证时不得提交 MiniMax Profile"
    );
    assert_eq!(
        state.values().minimax_tts_voice_id(),
        Some(&saved_voice),
        "拒绝提交后已保存 Voice 必须保持原值"
    );

    let expected = state
        .tts_picker_profile_generation()
        .expect("打开的选择器必须有草稿代际");
    assert_eq!(
        state.mark_tts_picker_minimax_validated(expected),
        Some(true),
        "当前草稿代际的验证结果必须被接受"
    );
    assert_eq!(
        state.tts_picker_minimax_readiness(),
        tts::RemoteTtsReadiness::Ready,
        "验证通过后手动 Voice ID 才达到 Ready"
    );
    assert!(
        state.commit_tts_picker_minimax_profile(),
        "已验证的手动 Voice ID 必须可以提交"
    );
    assert_eq!(
        state.values().minimax_tts_voice_id(),
        Some(&manual_voice),
        "提交后已保存 Profile 必须换成已验证的手动 Voice ID"
    );
}

fn write_reference_wav(path: &std::path::Path) {
    let sample_rate = 16_000u32;
    let duration_seconds = 3u32;
    let channels = 1u16;
    let frames = sample_rate * duration_seconds;
    let data_bytes = frames * u32::from(channels) * 2;
    let mut bytes = Vec::with_capacity((44 + data_bytes) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize((44 + data_bytes) as usize, 0);
    std::fs::write(path, bytes).expect("测试 WAV 应写入临时目录");
}
