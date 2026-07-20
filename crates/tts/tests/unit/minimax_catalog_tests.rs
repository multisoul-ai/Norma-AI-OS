use super::voice::MiniMaxVoiceApi;
use super::MiniMaxProvider;
use crate::{TtsError, TtsVoiceOrigin};
use std::sync::Arc;

struct FakeVoiceApi {
    response: Result<String, TtsError>,
}

impl MiniMaxVoiceApi for FakeVoiceApi {
    fn query_all(&self, _api_key: &str) -> Result<String, TtsError> {
        self.response.clone()
    }
}

/// MiniMax 账户音色被规范化为 System、Cloned、Generated 三个明确分组。
///
/// 数据构造（含关键数值的推导过程）：
///   system voices = 1
///   cloned voices = 1
///   generated voices = 1
///   total = 1 + 1 + 1 = 3
///
/// 执行过程：
///   1. 通过 Provider 公共 query_voices seam 注入成功的 get_voice 响应。
///   2. 读取规范化目录及每个分组。
///
/// 预期结果：
///   - 正断言：三个具体 Voice ID 各自在正确分组中。
///   - 负断言：Cloned 与 Generated 不会被错误归入 System。
#[test]
fn query_voices_normalizes_three_provider_groups_without_merging() {
    let response = r#"{
        "system_voice":[{"voice_id":"system-a","voice_name":"System A","description":["official"]}],
        "voice_cloning":[{"voice_id":"clone-a","description":[]}],
        "voice_generation":[{"voice_id":"generated-a","description":[]}],
        "base_resp":{"status_code":0,"status_msg":"success"}
    }"#;
    let provider = MiniMaxProvider::new_with_voice_api(
        "test-secret".to_string(),
        Arc::new(FakeVoiceApi {
            response: Ok(response.to_string()),
        }),
    )
    .expect("non-empty test credential should construct Provider");

    let catalog = provider
        .query_voices()
        .expect("valid get_voice fixture should normalize");

    assert_eq!(catalog.voices().len(), 3, "规范化目录应恰好包含三个音色");
    let system = catalog
        .voices()
        .iter()
        .find(|voice| voice.id().as_str() == "system-a")
        .expect("system-a 应存在");
    let cloned = catalog
        .voices()
        .iter()
        .find(|voice| voice.id().as_str() == "clone-a")
        .expect("clone-a 应存在");
    let generated = catalog
        .voices()
        .iter()
        .find(|voice| voice.id().as_str() == "generated-a")
        .expect("generated-a 应存在");
    assert_eq!(
        system.origin(),
        TtsVoiceOrigin::System,
        "system_voice 必须映射为 System"
    );
    assert_eq!(
        cloned.origin(),
        TtsVoiceOrigin::Cloned,
        "voice_cloning 必须映射为 Cloned"
    );
    assert_eq!(
        generated.origin(),
        TtsVoiceOrigin::Generated,
        "voice_generation 必须映射为 Generated"
    );
    assert_ne!(
        cloned.origin(),
        TtsVoiceOrigin::System,
        "Cloned 音色不得混入 System 分组"
    );
    assert_ne!(
        generated.origin(),
        TtsVoiceOrigin::System,
        "Generated 音色不得混入 System 分组"
    );
}

/// MiniMax Provider 的 Debug 输出不包含用户 API Key。
///
/// 数据构造（含关键数值的推导过程）：
///   secret occurrences allowed in Debug = 0
///
/// 执行过程：
///   1. 用明显的测试密钥构造 Provider。
///   2. 生成 Debug 字符串。
///
/// 预期结果：
///   - 正断言：输出仍可识别为 MiniMaxProvider。
///   - 负断言：密钥原文不出现。
#[test]
fn provider_debug_redacts_api_key() {
    let provider = MiniMaxProvider::new_with_voice_api(
        "secret-that-must-not-appear".to_string(),
        Arc::new(FakeVoiceApi {
            response: Ok("{}".to_string()),
        }),
    )
    .expect("non-empty test credential should construct Provider");

    let debug = format!("{provider:?}");

    assert!(
        debug.contains("MiniMaxProvider"),
        "脱敏 Debug 仍应标识 Provider 类型"
    );
    assert!(
        !debug.contains("secret-that-must-not-appear"),
        "Provider Debug 不得泄露 API Key"
    );
}
