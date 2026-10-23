use super::*;

/// 提供方标识与显示名稳定，且可由标识还原。
///
/// 数据构造：遍历 `Provider::ALL`。
/// 执行过程：对每个提供方取 `id()`，再用 `from_id` 还原。
/// 预期结果：
///   - 正断言：还原结果等于自身；ALL 共 2 项且 id 互不相同
///   - 负断言：未知 id 与大小写不符的 id 返回 None
#[test]
fn test_provider_ids_round_trip_and_are_unique() {
    assert_eq!(
        Provider::ALL.len(),
        2,
        "本期只内置 GitHub 与 Google 两个提供方"
    );
    for provider in Provider::ALL {
        assert_eq!(
            Provider::from_id(provider.id()),
            Some(provider),
            "id 应能还原为同一提供方"
        );
    }
    assert_ne!(
        Provider::GitHub.id(),
        Provider::Google.id(),
        "两个提供方的 id 必须不同"
    );
    assert_eq!(
        Provider::from_id("apple"),
        None,
        "未注册的提供方应返回 None"
    );
    assert_eq!(
        Provider::from_id("GitHub"),
        None,
        "id 区分大小写，只接受小写"
    );
    assert_eq!(
        Provider::GitHub.display_name(),
        "GitHub",
        "显示名按品牌写法"
    );
}
