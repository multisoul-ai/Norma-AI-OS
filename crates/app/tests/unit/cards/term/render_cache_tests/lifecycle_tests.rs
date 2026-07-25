use super::*;

/// TRC-8：retain live nodes 必须只释放已关闭节点，不得驱逐仍存活节点。
///
/// 数据构造（含关键数值的推导过程）：
///   cached nodes = A(201) + B(202) = 2 entries
///   live nodes after close = [B(202)] = 1 entry
///   expected removal = before(2) - live(1) = 1
///   B 的负对照 prepare marker = 83（若被错误驱逐才会执行）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别 resolve A→marker81、B→marker82，形成两个节点 entry。
///   2. 模拟 Runtime 删除 A，调用 retain_nodes([B])。
///   3. 检查 A/B entry，并再次以相同 B key resolve 负对照 marker83。
///
/// 预期结果：
///   - 正断言：cache len 从 2 降到 1，A 不存在、B 仍存在且命中 marker82。
///   - 正断言：B 的 prepared Arc 在 retain 前后相同。
///   - 负断言：retain 不得清空为 0，也不得执行 marker83。
#[test]
fn test_terminal_render_cache_removes_closed_node_without_evicting_live_node() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let metrics = terminal_render_metrics(1.0);
    let theme = Theme::paper_blue();
    let key_a = TerminalRenderKey::new(201, 21, metrics, theme, None);
    let key_b = TerminalRenderKey::new(202, 22, metrics, theme, None);

    let (frame_a, outcome_a) = cache.resolve(key_a, || 81);
    let (frame_b, outcome_b) = cache.resolve(key_b, || 82);
    let before_len = cache.len();
    cache.retain_nodes([202]);
    let after_len = cache.len();
    let retained_a = cache.entry_for_test(201);
    let retained_b = cache
        .entry_for_test(202)
        .expect("retain_nodes([B]) 后 B entry 必须仍存在");
    let (frame_b_after_retain, b_after_outcome) = cache.resolve(key_b, || 83);

    assert_eq!(
        outcome_a,
        TerminalCacheOutcome::Miss,
        "节点 A 首次 resolve 必须为 Miss"
    );
    assert_eq!(
        outcome_b,
        TerminalCacheOutcome::Miss,
        "节点 B 首次 resolve 必须为 Miss"
    );
    assert_eq!(before_len, 2, "retain 前 A/B 应形成恰好 2 个 cache entry");
    assert_eq!(
        after_len, 1,
        "关闭 A 后 retain live B 应把 cache 从 2 个 entry 收缩为 1 个"
    );
    assert_ne!(after_len, 0, "retain_nodes([B]) 不得把仍存活的 B 一并清空");
    assert!(
        retained_a.is_none(),
        "已关闭节点 A(201) 的 prepared frame 必须释放"
    );
    assert!(
        Arc::ptr_eq(&frame_b, &retained_b),
        "仍存活节点 B(202) 必须保留原 prepared frame Arc"
    );
    assert_eq!(
        b_after_outcome,
        TerminalCacheOutcome::Hit,
        "retain 后相同 B key 必须继续命中"
    );
    assert!(
        Arc::ptr_eq(&frame_b, &frame_b_after_retain),
        "retain 前后 B 的 prepared frame Arc 必须相同"
    );
    assert_eq!(*frame_b_after_retain, 82, "retain 后 B 必须保留 marker82");
    assert_ne!(
        *frame_b_after_retain, 83,
        "retain 后 B 不得被错误驱逐并执行 marker83 闭包"
    );
    assert_eq!(
        *frame_a, 81,
        "外部仍持有的 A Arc 应保持 marker81，证明删除只释放 cache 所有权"
    );
}
