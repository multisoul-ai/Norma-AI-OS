use super::*;

/// TLS-CRYPTO-1：应用网络初始化后必须能在多 rustls 后端共存时安全构建客户端配置。
///
/// 数据构造（含关键数值的推导过程）：
///   compiled crypto providers = 2（aws-lc-rs + ring，来自 workspace 统一 features）
///   explicitly selected defaults = 1（应用启动时只能安装一个进程级默认 Provider）
///   network requests = 0（构建 ClientConfig 即可触发原始崩溃，无需真实 API Key）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用应用公开的网络加密初始化 seam。
///   2. 查询进程级 CryptoProvider，再构建与 HTTPS 客户端相同的 rustls ClientConfig。
///   3. 捕获构建期间的 panic，区分正常返回与原始未选择 Provider 崩溃。
///
/// 预期结果：
///   - 正断言：初始化后进程必须存在明确的默认 CryptoProvider。
///   - 负断言：ClientConfig 构建不得再触发 “Could not automatically determine” panic。
#[test]
fn network_crypto_initialization_prevents_ambiguous_provider_panic() {
    install_network_crypto_provider();

    let build = std::panic::catch_unwind(rustls::ClientConfig::builder);
    assert!(
        build.is_ok(),
        "rustls ClientConfig 构建不得因 aws-lc-rs 与 ring 共存而 panic"
    );
    assert!(
        rustls::crypto::CryptoProvider::get_default().is_some(),
        "应用网络初始化后必须安装一个进程级默认 CryptoProvider"
    );
}
