use super::*;
use crate::canvas::text_selection::{TextPoint, TextRange};

/// TRC-3：terminal revision 变化必须替换 prepared frame，相同新 revision 随后应命中。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 103；revision sequence = 7 → 8 → 8
///   metrics/theme/selection changes = 0
///   expected prepares = rev7 miss(1) + rev8 miss(1) + rev8 hit(0) = 2
///   prepared markers = 41 / 42 / 43（43 是不应执行的负对照）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve revision 7，生成 marker 41。
///   2. 仅把 revision 改为 8，resolve 生成 marker 42。
///   3. 使用相同 revision 8 再 resolve，负对照闭包返回 marker 43。
///
/// 预期结果：
///   - 正断言：revision 7/8 都是 Miss 且 Arc 不同，第二次 revision 8 为 Hit。
///   - 正断言：最终 marker 为 42，prepare count 精确为 2。
///   - 负断言：缓存不得忽略 revision，也不得在稳定 revision 上永远 Miss。
#[test]
fn test_terminal_render_cache_misses_when_revision_changes() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let metrics = terminal_render_metrics(1.0);
    let theme = Theme::paper_blue();
    let revision_7 = TerminalRenderKey::new(103, 7, metrics, theme, None);
    let revision_8 = TerminalRenderKey::new(103, 8, metrics, theme, None);

    let (first, first_outcome) = cache.resolve(revision_7, || 41);
    let (second, second_outcome) = cache.resolve(revision_8, || 42);
    let (third, third_outcome) = cache.resolve(revision_8, || 43);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "revision 7 首次 resolve 必须为 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Miss,
        "revision 从 7 变为 8 时必须失效为 Miss"
    );
    assert!(
        !Arc::ptr_eq(&first, &second),
        "revision 变化必须替换 prepared frame Arc"
    );
    assert_eq!(
        third_outcome,
        TerminalCacheOutcome::Hit,
        "相同 revision 8 的负对照 resolve 必须命中"
    );
    assert!(
        Arc::ptr_eq(&second, &third),
        "稳定 revision 8 必须复用第二份 prepared frame"
    );
    assert_eq!(
        *third, 42,
        "稳定 revision 8 必须保留 marker 42，不能执行 marker 43 闭包"
    );
    assert_ne!(*third, 43, "稳定 revision 8 不得退化为永远 Miss");
    assert_eq!(
        cache.prepare_count(),
        2,
        "revision 7 与 8 各 prepare 一次，总数必须为 2"
    );
}

/// TRC-4：zoom 导致 terminal metrics 变化时必须失效；稳定 metrics 随后应命中。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 104；revision = 10
///   zoom 1.0 → 13px font / 16px line / 8px cell
///   zoom 1.25 → 16.25px font / 20px line / 10px cell
///   expected prepares = zoom1 miss(1) + zoom1.25 miss(1) + same zoom hit(0) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 固定 node/revision/theme/selection，以 zoom 1.0 resolve marker 51。
///   2. 仅把 metrics 改为 zoom 1.25，resolve marker 52。
///   3. 用相同 zoom 1.25 再 resolve，负对照闭包返回 marker 53。
///
/// 预期结果：
///   - 正断言：metrics 改变为 Miss/新 Arc，稳定 metrics 为 Hit/同 Arc。
///   - 正断言：最终 marker=52 且 prepare count=2。
///   - 负断言：不能忽略 zoom，也不能让稳定 zoom 重复 prepare marker 53。
#[test]
fn test_terminal_render_cache_misses_when_metrics_change_after_zoom() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let theme = Theme::paper_blue();
    let zoom_1 = TerminalRenderKey::new(104, 10, terminal_render_metrics(1.0), theme, None);
    let zoom_1_25 = TerminalRenderKey::new(104, 10, terminal_render_metrics(1.25), theme, None);

    let (first, first_outcome) = cache.resolve(zoom_1, || 51);
    let (second, second_outcome) = cache.resolve(zoom_1_25, || 52);
    let (third, third_outcome) = cache.resolve(zoom_1_25, || 53);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "zoom 1.0 首次 resolve 必须为 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Miss,
        "zoom 从 1.0 变为 1.25 后 metrics 变化必须触发 Miss"
    );
    assert!(
        !Arc::ptr_eq(&first, &second),
        "metrics 变化必须替换 prepared frame Arc"
    );
    assert_eq!(
        third_outcome,
        TerminalCacheOutcome::Hit,
        "相同 zoom 1.25 的负对照 resolve 必须命中"
    );
    assert!(
        Arc::ptr_eq(&second, &third),
        "稳定 metrics 必须复用 zoom 1.25 的 prepared frame"
    );
    assert_eq!(*third, 52, "稳定 metrics 必须保留 marker 52");
    assert_ne!(*third, 53, "稳定 metrics 不得再次执行 marker 53 闭包");
    assert_eq!(
        cache.prepare_count(),
        2,
        "两组不同 metrics 应各 prepare 一次，总数必须为 2"
    );
}

/// TRC-5：terminal 相关主题 token 变化必须失效；相同新主题随后应命中。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 105；revision = 11；zoom = 1.0；selection=None
///   theme sequence = PaperBlue → ForestMint → ForestMint
///   terminal key token changes ≥ term_fg/term_bg/focused_stroke
///   expected prepares = old theme miss(1) + new theme miss(1) + same theme hit(0) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 PaperBlue resolve marker 61。
///   2. 仅改为 ForestMint resolve marker 62。
///   3. 相同 ForestMint 再 resolve，负对照闭包返回 marker 63。
///
/// 预期结果：
///   - 正断言：主题 token 变化为 Miss/新 Arc，稳定主题为 Hit/同 Arc。
///   - 正断言：最终 marker=62 且 prepare count=2。
///   - 负断言：不能忽略 terminal theme，也不能让稳定主题重复 prepare。
#[test]
fn test_terminal_render_cache_misses_when_terminal_theme_tokens_change() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let metrics = terminal_render_metrics(1.0);
    let paper = TerminalRenderKey::new(105, 11, metrics, Theme::paper_blue(), None);
    let forest = TerminalRenderKey::new(105, 11, metrics, Theme::forest_mint(), None);

    let (first, first_outcome) = cache.resolve(paper, || 61);
    let (second, second_outcome) = cache.resolve(forest, || 62);
    let (third, third_outcome) = cache.resolve(forest, || 63);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "PaperBlue 首次 resolve 必须为 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Miss,
        "切换到 ForestMint 后 terminal 主题 token 变化必须触发 Miss"
    );
    assert!(
        !Arc::ptr_eq(&first, &second),
        "terminal theme token 变化必须替换 prepared frame Arc"
    );
    assert_eq!(
        third_outcome,
        TerminalCacheOutcome::Hit,
        "相同 ForestMint 的负对照 resolve 必须命中"
    );
    assert!(
        Arc::ptr_eq(&second, &third),
        "稳定主题必须复用 ForestMint prepared frame"
    );
    assert_eq!(*third, 62, "稳定主题必须保留 marker 62");
    assert_ne!(*third, 63, "稳定主题不得再次执行 marker 63 闭包");
    assert_eq!(
        cache.prepare_count(),
        2,
        "两个不同 terminal 主题应各 prepare 一次，总数必须为 2"
    );
}

/// TRC-6：terminal 文本 selection 变化必须失效；稳定 selection 随后应命中。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 106；revision = 12；zoom = 1.0；theme = PaperBlue
///   selection sequence = None → row0[col0..col2] → same range
///   selected columns = end(2) - start(0) = 2
///   expected prepares = no-selection miss(1) + selection miss(1) + same selection hit(0) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 无 selection resolve marker 71。
///   2. 仅增加 row0[col0..col2] selection，resolve marker 72。
///   3. 相同 selection 再 resolve，负对照闭包返回 marker 73。
///
/// 预期结果：
///   - 正断言：selection 变化为 Miss/新 Arc，稳定 selection 为 Hit/同 Arc。
///   - 正断言：最终 marker=72 且 prepare count=2。
///   - 负断言：不能复用无高亮帧，也不能在相同 selection 上重复 prepare。
#[test]
fn test_terminal_render_cache_misses_when_selection_changes() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let metrics = terminal_render_metrics(1.0);
    let theme = Theme::paper_blue();
    let range = TextRange::new(TextPoint::new(0, 0), TextPoint::new(0, 2));
    let no_selection = TerminalRenderKey::new(106, 12, metrics, theme, None);
    let selected = TerminalRenderKey::new(106, 12, metrics, theme, Some(range));

    let (first, first_outcome) = cache.resolve(no_selection, || 71);
    let (second, second_outcome) = cache.resolve(selected, || 72);
    let (third, third_outcome) = cache.resolve(selected, || 73);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "无 selection 的首次 resolve 必须为 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Miss,
        "新增两列 selection highlight 后必须触发 Miss"
    );
    assert!(
        !Arc::ptr_eq(&first, &second),
        "selection 变化必须替换 prepared frame Arc"
    );
    assert_eq!(
        third_outcome,
        TerminalCacheOutcome::Hit,
        "相同 selection 的负对照 resolve 必须命中"
    );
    assert!(
        Arc::ptr_eq(&second, &third),
        "稳定 selection 必须复用带高亮的 prepared frame"
    );
    assert_eq!(*third, 72, "稳定 selection 必须保留 marker 72");
    assert_ne!(*third, 73, "稳定 selection 不得再次执行 marker 73 闭包");
    assert_eq!(
        cache.prepare_count(),
        2,
        "无 selection 与有 selection 应各 prepare 一次，总数必须为 2"
    );
}
