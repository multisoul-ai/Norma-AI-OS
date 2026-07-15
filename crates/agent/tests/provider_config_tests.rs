use agent::{ConfiguredProvider, ProviderConfig, ProviderProtocol};

/// PC-1：统一 Provider 工厂应接受 HTTPS OpenAI-compatible 配置并保持密钥脱敏。
///
/// 数据构造：protocol=OpenAiCompatible，base_url=https://api.openai.com/v1，
/// model=gpt-test，api_key=top-secret。
/// 执行过程：校验配置、构造 ConfiguredProvider，再格式化配置 Debug 文本。
/// 预期结果：配置和构造均成功；Debug 包含 redacted，且不包含明文密钥。
#[test]
fn openai_https_config_builds_without_exposing_api_key() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://api.openai.com/v1",
        "gpt-test",
        Some("top-secret".to_string()),
    );

    config
        .validate()
        .expect("HTTPS OpenAI-compatible 配置应通过校验");
    let provider = ConfiguredProvider::new(config.clone())
        .expect("有效 OpenAI-compatible 配置应构造统一 Provider");
    let debug = format!("{config:?}");

    assert_eq!(
        provider.protocol(),
        ProviderProtocol::OpenAiCompatible,
        "统一 Provider 应保留 OpenAI-compatible 协议选择"
    );
    assert!(debug.contains("<redacted>"), "Debug 应明确显示密钥已脱敏");
    assert!(
        !debug.contains("top-secret"),
        "Debug 不得包含 OpenAI API Key 明文"
    );
}

/// PC-2：任何 Provider 缺少 model 时都应在发起网络请求前失败。
///
/// 数据构造：Ollama base_url 为默认本地地址，model 仅含空白，api_key=None。
/// 执行过程：调用 ProviderConfig::validate。
/// 预期结果：返回包含 model 的错误；不得错误地接受空模型。
#[test]
fn provider_config_rejects_blank_model_before_network_io() {
    let config = ProviderConfig::new(
        ProviderProtocol::Ollama,
        "http://127.0.0.1:11434",
        "   ",
        None,
    );

    let error = config
        .validate()
        .expect_err("空 model 必须在网络请求前失败");
    let message = error.to_string();

    assert!(message.contains("model"), "错误信息应明确指出 model 缺失");
    assert_ne!(message, "", "配置错误不得返回空消息");
}
