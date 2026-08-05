use tts::{
    builtin_provider_registry, evaluate_remote_readiness, RemoteAccountStatus, RemoteTtsReadiness,
    RemoteTtsValidation, TtsModelId, TtsProviderConfiguration, TtsProviderId, TtsProviderProfile,
    TtsValidatedSelection, TtsVoiceId, VoiceSelectionProvenance, LOCAL_TTS_PROVIDER_ID,
    MINIMAX_SPEECH_HD_MODEL_ID, MINIMAX_SPEECH_TURBO_MODEL_ID, MINIMAX_TTS_PROVIDER_ID,
};

fn minimax_profile(voice_id: &str, provenance: VoiceSelectionProvenance) -> TtsProviderProfile {
    TtsProviderProfile::new(
        TtsProviderId::parse(MINIMAX_TTS_PROVIDER_ID).expect("MiniMax provider ID should parse"),
        TtsModelId::parse(MINIMAX_SPEECH_TURBO_MODEL_ID)
            .expect("MiniMax Turbo model ID should parse"),
        TtsVoiceId::parse(voice_id).expect("fixture Voice ID should parse"),
        provenance,
    )
}

/// 内置 Provider 注册表只公开首期支持的本地与 MiniMax 能力。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in providers = local(1) + minimax(1) = 2
///   MiniMax models     = turbo(1) + hd(1) = 2
///
/// 执行过程：
///   1. 读取只读内置注册表。
///   2. 按稳定 Provider ID 查找 local 与 minimax。
///   3. 读取 MiniMax 的静态模型目录。
///
/// 预期结果：
///   - 正断言：local、minimax、Turbo 与 HD 均存在。
///   - 负断言：不存在第三个 Provider，也不存在旧版 2.6 模型。
#[test]
fn builtin_registry_exposes_only_supported_providers_and_minimax_models() {
    let registry = builtin_provider_registry();

    assert_eq!(
        registry.providers().len(),
        2,
        "首期内置注册表应恰好包含 local 与 minimax 两个 Provider"
    );
    let local = registry
        .provider(LOCAL_TTS_PROVIDER_ID)
        .expect("local Provider 应存在于内置注册表");
    let minimax = registry
        .provider(MINIMAX_TTS_PROVIDER_ID)
        .expect("minimax Provider 应存在于内置注册表");
    assert_eq!(
        local.id().as_str(),
        LOCAL_TTS_PROVIDER_ID,
        "本地 Provider 必须使用稳定 ID local"
    );
    assert_eq!(
        local.configuration(),
        TtsProviderConfiguration::OnDeviceModel,
        "On-device 注册项必须声明本地模型配置要求"
    );
    assert_eq!(
        minimax.configuration(),
        TtsProviderConfiguration::ApiKeyWithVoiceCatalog,
        "MiniMax 注册项必须声明 API Key 与账户 Voice 目录要求"
    );
    assert_eq!(
        minimax.models().len(),
        2,
        "MiniMax 首期模型目录应恰好包含 Turbo 与 HD"
    );
    assert!(
        minimax
            .models()
            .iter()
            .any(|model| model.id().as_str() == MINIMAX_SPEECH_TURBO_MODEL_ID),
        "MiniMax 模型目录应包含默认 Turbo 模型"
    );
    assert!(
        minimax
            .models()
            .iter()
            .any(|model| model.id().as_str() == MINIMAX_SPEECH_HD_MODEL_ID),
        "MiniMax 模型目录应包含可选 HD 模型"
    );
    assert!(
        minimax
            .models()
            .iter()
            .all(|model| !model.id().as_str().contains("2.6")),
        "MiniMax 首期不得暴露未纳入范围的 2.6 模型"
    );
    assert!(
        registry.provider("runtime-plugin").is_none(),
        "内置注册表不得凭任意字符串产生运行时 Provider"
    );
}

/// 目录中的 Voice ID 在账户验证成功后直接达到 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog voices = selected(1) + unrelated(1) = 2
///   manual validations = 0
///
/// 执行过程：
///   1. 构造来源为 Catalog 的 MiniMax profile。
///   2. 提供 Ready 账户状态与包含该 Voice ID 的目录。
///   3. 通过公共 readiness seam 求值。
///
/// 预期结果：
///   - 正断言：目录精确命中时状态为 Ready。
///   - 负断言：不要求额外的手动 Preview 验证。
#[test]
fn catalog_voice_is_ready_after_authenticated_catalog_match() {
    let profile = minimax_profile("system-voice-a", VoiceSelectionProvenance::Catalog);
    let validation = RemoteTtsValidation::new(
        RemoteAccountStatus::Ready,
        "keychain:minimax:primary",
        vec![
            TtsVoiceId::parse("system-voice-a").expect("selected catalog Voice should parse"),
            TtsVoiceId::parse("system-voice-b").expect("unrelated catalog Voice should parse"),
        ],
        None,
    );

    let readiness = evaluate_remote_readiness(Some(&profile), &validation);

    assert_eq!(
        readiness,
        RemoteTtsReadiness::Ready,
        "账户验证成功且目录精确命中时应直接 Ready"
    );
    assert_ne!(
        readiness,
        RemoteTtsReadiness::VoiceValidationRequired,
        "目录内 Voice 不应被错误要求执行强制 Preview"
    );
}

/// 目录外手填 Voice ID 必须完成绑定当前配置的显式验证。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog voices = 1（不含 manual-voice）
///   matching tuple = credential(1) + model(1) + voice(1)
///
/// 执行过程：
///   1. 构造来源为 Manual 的目录外 Voice ID。
///   2. 先在没有 Preview 证明时求值。
///   3. 再加入完全匹配的已验证三元组并重新求值。
///
/// 预期结果：
///   - 正断言：验证前要求 Voice Validation，验证后 Ready。
///   - 负断言：字段齐全本身不能让目录外 ID 直接 Ready。
#[test]
fn manual_voice_requires_matching_preview_validation_before_ready() {
    let profile = minimax_profile("manual-voice", VoiceSelectionProvenance::Manual);
    let without_preview = RemoteTtsValidation::new(
        RemoteAccountStatus::Ready,
        "keychain:minimax:primary",
        vec![TtsVoiceId::parse("catalog-voice").expect("catalog Voice should parse")],
        None,
    );

    let before = evaluate_remote_readiness(Some(&profile), &without_preview);
    assert_eq!(
        before,
        RemoteTtsReadiness::VoiceValidationRequired,
        "目录外手填 Voice ID 在显式验证前必须保持待验证状态"
    );
    assert_ne!(
        before,
        RemoteTtsReadiness::Ready,
        "非空 Voice ID 与有效凭据不足以证明供应商会接受该 ID"
    );

    let validated = TtsValidatedSelection::from_profile("keychain:minimax:primary", &profile);
    let with_preview = RemoteTtsValidation::new(
        RemoteAccountStatus::Ready,
        "keychain:minimax:primary",
        vec![TtsVoiceId::parse("catalog-voice").expect("catalog Voice should parse")],
        Some(validated),
    );
    let after = evaluate_remote_readiness(Some(&profile), &with_preview);

    assert_eq!(
        after,
        RemoteTtsReadiness::Ready,
        "相同凭据、模型与 Voice ID 的显式验证成功后应 Ready"
    );
    assert_ne!(
        after,
        RemoteTtsReadiness::VoiceValidationRequired,
        "有效三元组不应继续显示待验证状态"
    );
}

/// 手填 Voice 的验证证明不能跨凭据、模型或 Voice ID 复用。
///
/// 数据构造（含关键数值的推导过程）：
///   original tuple = credential-a + turbo + manual-voice
///   changed tuple  = credential-b + turbo + manual-voice
///   differing fields = 1（credential）
///
/// 执行过程：
///   1. 用 credential-a 为 profile 生成已验证三元组。
///   2. 在当前凭据已经变为 credential-b 的上下文中求值。
///
/// 预期结果：
///   - 正断言：状态回到 VoiceValidationRequired。
///   - 负断言：旧凭据产生的证明不能让新凭据配置 Ready。
#[test]
fn manual_voice_validation_is_invalidated_when_credential_changes() {
    let profile = minimax_profile("manual-voice", VoiceSelectionProvenance::Manual);
    let stale_validation = TtsValidatedSelection::from_profile("credential-a", &profile);
    let validation = RemoteTtsValidation::new(
        RemoteAccountStatus::Ready,
        "credential-b",
        Vec::new(),
        Some(stale_validation),
    );

    let readiness = evaluate_remote_readiness(Some(&profile), &validation);

    assert_eq!(
        readiness,
        RemoteTtsReadiness::VoiceValidationRequired,
        "凭据变化后应要求重新验证手填 Voice ID"
    );
    assert_ne!(
        readiness,
        RemoteTtsReadiness::Ready,
        "旧凭据的验证证明不得跨凭据复用"
    );
}
