use agent::{ProviderConfig, ProviderProtocol};

/// PC-1：统一 Provider 工厂应接受 HTTPS OpenAI-compatible 配置并保持密钥脱敏。
///
/// 数据构造：protocol=OpenAiCompatible，base_url=https://api.openai.com/v1，
/// model=gpt-test，api_key=top-secret。
/// 执行过程：校验配置，再格式化配置 Debug 文本。
/// 预期结果：配置成功；Debug 包含 redacted，且不包含明文密钥。
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
    let debug = format!("{config:?}");

    assert_eq!(
        config.protocol(),
        ProviderProtocol::OpenAiCompatible,
        "Rig ModelFactory 配置应保留 OpenAI-compatible 协议选择"
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

/// PC-3：Provider 可声明模型上下文窗口，供会话按真实容量裁剪协议历史。
///
/// 数据构造（含关键数值的推导过程）：
///   requested context = 32,768 tokens
///   expected stored context = 32,768 tokens
///   api key occurrences in Debug = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 OpenAI-compatible 配置并声明上下文窗口
///   2. 读取上下文窗口 getter
///   3. 检查脱敏 Debug 输出
///
/// 预期结果：
///   - 正断言：配置保留 32,768 token 上下文容量
///   - 负断言：新增字段不得导致 Debug 泄露 API Key
#[test]
fn provider_config_carries_context_window_without_exposing_api_key() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider.example/v1",
        "model-a",
        Some("context-secret".to_string()),
    )
    .with_context_window_tokens(32_768);
    let debug = format!("{config:?}");

    assert_eq!(
        config.context_window_tokens(),
        Some(32_768),
        "配置必须保留声明的 32,768 token 上下文窗口"
    );
    assert!(
        debug.contains("32768"),
        "脱敏 Debug 应显示可诊断的上下文窗口值"
    );
    assert!(
        !debug.contains("context-secret"),
        "上下文配置不得破坏 API Key 脱敏"
    );
    assert_ne!(
        config.context_window_tokens(),
        Some(1),
        "有效上下文窗口不得错误收缩为最小值 1"
    );
}
