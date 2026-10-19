use super::*;

fn saved_key(store: &storage::SettingsStore, account: &str, secret: &str) {
    store
        .set_string("voice.gpt_live.credential_ref", account)
        .expect("保存Live引用");
    store
        .set_string(&format!("credential.agent-provider.{account}"), secret)
        .expect("保存隔离测试Key");
}

/// 场景：只保存Live Key即可配置项目工具，旧全局供应商和自定义地址不能接收到Live Key。
/// 构造：隔离SQLite，保存Live Key及指向无效地址的全局Ollama/OpenAI设置。
/// 执行：调用实际工具后台配置加载入口，再替换全局供应商重复加载。
/// 预期：始终使用官方OpenAI地址、默认工具模型及同一Live Key；全局配置保持原值。
#[test]
fn live_backend_uses_only_live_key_and_official_defaults() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    saved_key(&store, "gpt-live-desktop-fixture", "fixture-live-key");
    store
        .set_string(
            "agent.provider.openai.base_url",
            "https://untrusted.invalid/v1",
        )
        .expect("旧自定义地址");
    for provider in ["ollama", "openai"] {
        store
            .set_string("agent.provider.active", provider)
            .expect("旧供应商");
        let config = load_config(&store, "gpt-live-desktop-fixture").expect("无需额外后台设置");
        assert_eq!(
            config.protocol(),
            agent::ProviderProtocol::OpenAiCompatible,
            "工具使用官方兼容协议"
        );
        assert_eq!(
            config.endpoint(),
            "https://api.openai.com/v1",
            "绝不将Live Key发到用户旧自定义地址"
        );
        assert_eq!(
            config.model(),
            "gpt-5.4-mini",
            "工具默认模型来自已核对的官方模型能力"
        );
        assert_eq!(
            config.api_key(),
            Some("fixture-live-key"),
            "复用Live Key，不借用其他服务凭据"
        );
        assert!(
            !format!("{config:?}").contains("fixture-live-key"),
            "调试输出必须脱敏"
        );
        assert_eq!(
            store
                .get_string("agent.provider.active")
                .expect("读取全局供应商")
                .as_deref(),
            Some(provider),
            "不能改写全局Norma供应商"
        );
    }
}

/// 场景：Key删除、替换或引用损坏时，后台不能继续使用旧Key或读取其他供应商secret。
/// 构造：隔离SQLite中的旧Live Key，新Live Key，以及外来豆包账号。
/// 执行：先成功加载→替换活动引用→删除新Key→尝试外来账号。
/// 预期：过期、缺失和外来引用均失败；新活动Key可以生效，无隐含回退。
#[test]
fn live_backend_rejects_stale_missing_and_foreign_credentials() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    saved_key(&store, "gpt-live-desktop-old", "fixture-old");
    assert!(
        load_config(&store, "gpt-live-desktop-old").is_ok(),
        "有效活动Key可用"
    );
    saved_key(&store, "gpt-live-desktop-new", "fixture-new");
    assert!(
        load_config(&store, "gpt-live-desktop-old").is_err(),
        "替换后禁止使用旧Key"
    );
    assert_eq!(
        load_config(&store, "gpt-live-desktop-new")
            .expect("新Key")
            .api_key(),
        Some("fixture-new"),
        "后续请求读取新活动Key"
    );
    store
        .delete_string("credential.agent-provider.gpt-live-desktop-new")
        .expect("移除secret");
    assert!(
        load_config(&store, "gpt-live-desktop-new").is_err(),
        "缺失secret不回退到旧Key"
    );
    saved_key(&store, "doubao-desktop", "fixture-foreign");
    assert!(
        load_config(&store, "doubao-desktop").is_err(),
        "禁止读取外来账号"
    );
}
