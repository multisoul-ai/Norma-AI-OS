//! PKCE（RFC 7636，S256）与 OAuth `state` 的生成和校验。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fmt;

/// 一次登录尝试的 PKCE 对。verifier 只在令牌交换时出现。
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    /// 由 32 字节熵构造（43 字符 verifier），便于测试注入确定熵。
    pub fn from_entropy(entropy: &[u8; 32]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(entropy);
        let challenge = challenge_for(&verifier);
        Self {
            verifier,
            challenge,
        }
    }

    pub fn generate() -> Self {
        let mut entropy = [0u8; 32];
        OsRng.fill_bytes(&mut entropy);
        Self::from_entropy(&entropy)
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

impl fmt::Debug for Pkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pkce")
            .field("verifier", &"<redacted>")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// `BASE64URL(SHA256(verifier))`。
pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 256 位随机 `state`。
pub fn random_state() -> String {
    let mut entropy = [0u8; 32];
    OsRng.fill_bytes(&mut entropy);
    URL_SAFE_NO_PAD.encode(entropy)
}

/// 常量时间比较；期望值或实际值为空一律拒绝。
pub fn verify_state(expected: &str, actual: &str) -> bool {
    if expected.is_empty() || actual.is_empty() || expected.len() != actual.len() {
        return false;
    }
    expected
        .bytes()
        .zip(actual.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}
