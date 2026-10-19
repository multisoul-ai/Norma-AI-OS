use super::*;

/// AC12/AC13：Live配置只保存独立凭据引用与合法音色，不能继承豆包或后台的Key。
/// 数据构造：独立opaque账号、cedar音色、空账号及未知音色；无任何真实secret。
/// 执行过程：从持久值恢复并读取公开配置，再检查默认值与Debug输出。
/// 预期结果：引用与音色正确，未知音色回到官方marin，调试文本不显示账号内容。
#[test]
fn live_profile_restores_independent_reference_and_supported_voice() {
    let profile = GptLiveProfile::from_stored(
        Some(" gpt-live-desktop-credential ".into()),
        Some("cedar".into()),
    );
    assert_eq!(
        profile.credential_ref(),
        Some("gpt-live-desktop-credential"),
        "Live有独立账号引用"
    );
    assert_eq!(profile.voice(), "cedar", "恢复明确选择的官方音色");
    assert!(
        !format!("{profile:?}").contains("gpt-live-desktop-credential"),
        "日志不暴露凭据引用内容"
    );
    let fallback = GptLiveProfile::from_stored(Some(" ".into()), Some("invalid-voice".into()));
    assert_eq!(fallback.credential_ref(), None, "空引用不能表示已配置");
    assert_eq!(fallback.voice(), "marin", "未知音色恢复为官方默认值");
    assert_eq!(
        GptLiveProfile::default().voice(),
        "marin",
        "新配置使用官方默认音色"
    );
}

/// AC12：损坏的Live引用不得把其他供应商secret当成Live凭据使用。
/// 数据构造：正常Live账号和豆包/兼容OpenAI账号引用，不含真实secret。
/// 执行过程：分别恢复Profile并读取对外凭据引用。
/// 预期结果：仅Live命名空间可用于连接；外来原始值仍留在内部供安全替换修复。
#[test]
fn live_profile_does_not_use_foreign_provider_accounts() {
    for account in [
        "doubao-desktop",
        "openai-compatible-fixture",
        "gpt-live-desktop",
    ] {
        let profile = GptLiveProfile::from_stored(Some(account.into()), None);
        assert_eq!(
            profile.credential_ref(),
            None,
            "外来账号不能作为Live已配置凭据"
        );
        assert_eq!(
            profile.credential_ref.as_deref(),
            Some(account),
            "保留原始引用用于条件修复，不自动删外来secret"
        );
    }
}
