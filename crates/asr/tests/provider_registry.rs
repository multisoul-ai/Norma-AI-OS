use asr::{
    builtin_asr_providers, builtin_recognizer_factory, AsrProviderId, AzureSpeechProfile,
    BuiltinAsrProfile,
};

/// AP-1：内置 ASR 注册表应只包含 Local Sherpa 与 Cloud Azure。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in provider count = 2（1 个本地 + 1 个云端）
///   Local Sherpa = 支持 partial、无云端 60 秒上限
///   Cloud Azure = 不支持 partial、单次上限 60 秒
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取公开内置 Provider 注册表
///   2. 按稳定 ID 查找 Local Sherpa 和 Cloud Azure
///   3. 读取两者的能力描述
///
/// 预期结果：
///   - 两个具体 Provider 都应存在且分类正确
///   - Local 应支持 partial，Azure 不应支持 partial
///   - Azure 上限应为 60 秒，Local 不应继承该限制
///   - 不应出现第三个动态或未知 Provider
#[test]
fn builtin_registry_exposes_local_sherpa_and_cloud_azure_capabilities() {
    let providers = builtin_asr_providers();
    let local = providers
        .iter()
        .find(|provider| provider.id() == AsrProviderId::LocalSherpa)
        .expect("内置 ASR 注册表应包含 Local Sherpa");
    let azure = providers
        .iter()
        .find(|provider| provider.id() == AsrProviderId::CloudAzure)
        .expect("内置 ASR 注册表应包含 Cloud Azure");

    assert_eq!(providers.len(), 2, "首版注册表应恰好包含两个内置 Provider");
    assert!(
        local.capabilities().partial_results(),
        "Local Sherpa 应保留现有 streaming partial 能力"
    );
    assert_eq!(
        local.capabilities().max_utterance_seconds(),
        None,
        "Local Sherpa 不应继承 Azure 的 60 秒上限"
    );
    assert!(
        !azure.capabilities().partial_results(),
        "Azure 短音频 REST 首版不应宣称支持 partial"
    );
    assert_eq!(
        azure.capabilities().max_utterance_seconds(),
        Some(60),
        "Azure 短音频 REST 单次上限应为 60 秒"
    );
    assert!(
        providers.iter().all(|provider| matches!(
            provider.id(),
            AsrProviderId::LocalSherpa | AsrProviderId::CloudAzure
        )),
        "注册表不得包含动态或未知 Provider"
    );
}

/// AP-2：内置工厂注册表应在 ASR 层解析 Azure profile 与通用凭据 resolver。
///
/// 数据构造（含关键数值的推导过程）：
///   selected profile = Cloud Azure × 1
///   credential resolver calls = 1
///   expected max duration = 60 s
///   missing-secret attempts = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 提交不含明文密钥的 Azure profile，并由通用 resolver 返回 fake secret
///   2. 读取注册表创建的 recognizer factory 能力
///   3. 对同一 profile 使用返回 None 的 resolver，再次尝试创建
///
/// 预期结果：
///   - 正断言：ASR 注册表创建 Azure factory，并声明 60 秒、无 partial
///   - 负断言：凭据缺失时不得创建 factory，也不得回退到 Local Sherpa
#[test]
fn builtin_factory_registry_resolves_azure_without_app_vendor_branch() {
    let profile = || {
        BuiltinAsrProfile::CloudAzure(AzureSpeechProfile {
            region: "eastus".to_string(),
            locale: "zh-CN".to_string(),
            credential_ref: "opaque-keychain-reference".to_string(),
        })
    };

    let factory = builtin_recognizer_factory(profile(), |reference| {
        assert_eq!(
            reference, "opaque-keychain-reference",
            "注册表应把 opaque reference 原样交给通用凭据 resolver"
        );
        Ok(Some("fake-registry-secret".to_string()))
    })
    .expect("有效 profile 和凭据应创建 Azure factory");
    let missing = builtin_recognizer_factory(profile(), |_reference| Ok(None));

    assert!(
        !factory.capabilities().partial_results(),
        "注册表创建的 Azure factory 不得伪造 partial 能力"
    );
    assert_eq!(
        factory.capabilities().max_utterance_seconds(),
        Some(60),
        "注册表创建的 Azure factory 必须保留 60 秒上限"
    );
    assert!(
        missing.is_err(),
        "凭据 resolver 返回 None 时不得创建 Azure 或自动回退 Local factory"
    );
}
