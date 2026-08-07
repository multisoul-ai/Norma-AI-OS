/// 为进程内所有 rustls 客户端安装明确的加密 Provider。
pub(crate) fn install_network_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return;
    }
    let install = rustls::crypto::ring::default_provider().install_default();
    if install.is_err() && rustls::crypto::CryptoProvider::get_default().is_none() {
        panic!("failed to install the process-level rustls CryptoProvider");
    }
}

#[cfg(test)]
#[path = "../tests/unit/network_crypto_tests.rs"]
mod network_crypto_tests;
