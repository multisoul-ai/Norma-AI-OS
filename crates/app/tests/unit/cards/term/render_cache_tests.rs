use super::*;
use crate::cards::term::cursor::{resolve_cursor_style, TerminalCursorAnimation};
use crate::cards::term::paint_style::TerminalGlassStyle;
use crate::cards::term::view::terminal_render_metrics;
use crate::motion::working::{resolve_working_visual, WorkingVisualInput};
use std::sync::Arc;
use ui::Theme;

#[path = "render_cache_tests/invalidation_tests.rs"]
mod invalidation_tests;
#[path = "render_cache_tests/lifecycle_tests.rs"]
mod lifecycle_tests;
#[path = "render_cache_tests/shaping_tests.rs"]
mod shaping_tests;

/// TRC-1：相同节点、revision 与渲染输入必须复用同一 prepared frame。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 101
///   terminal revision = 7
///   zoom = 1.0 → metrics = 13px font / 16px line / 8px cell
///   resolve calls with identical key = 2
///   expected prepares = first miss(1) + second hit(0) = 1
///   first prepared marker = 11；第二个闭包的负对照 marker = 22
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 node 101/revision 7/固定 metrics/theme/无 selection 构造 key。
///   2. 第一次 resolve，prepare 闭包返回 marker 11。
///   3. 使用完全相同的 key 第二次 resolve，负对照闭包会返回 marker 22。
///   4. 比较 outcome、Arc 身份、实际 marker 与累计 prepare count。
///
/// 预期结果：
///   - 正断言：第一次为 Miss、第二次为 Hit，且两次 Arc 身份相同。
///   - 正断言：第二次仍读到 marker 11，prepare count 精确为 1。
///   - 负断言：第二次不得执行闭包并替换成 marker 22，也不得再次返回 Miss。
#[test]
fn test_terminal_render_cache_reuses_prepared_frame_for_same_revision_and_inputs() {
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let key = TerminalRenderKey::new(
        101,
        7,
        terminal_render_metrics(1.0),
        Theme::paper_blue(),
        None,
    );

    let (first, first_outcome) = cache.resolve(key, || 11);
    let (second, second_outcome) = cache.resolve(key, || 22);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "首次 resolve 没有既有 entry，必须返回 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Hit,
        "相同 key 的第二次 resolve 必须返回 Hit"
    );
    assert!(
        Arc::ptr_eq(&first, &second),
        "相同 key 必须复用同一 prepared frame Arc"
    );
    assert_eq!(*second, 11, "cache hit 必须保留首次 prepared marker 11");
    assert_ne!(
        *second, 22,
        "cache hit 不得执行第二个 prepare 闭包并替换为 marker 22"
    );
    assert_eq!(
        cache.prepare_count(),
        1,
        "两次相同 key resolve 只允许首次 prepare，共计 1 次"
    );
    assert_ne!(
        second_outcome,
        TerminalCacheOutcome::Miss,
        "第二次 resolve 不得退化为永远 Miss"
    );
}

/// TRC-2：cursor 与工作蛇动画 phase 变化不得使 terminal prepared frame 失效。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal key：node=102，revision=9，zoom=1.0，selection=None
///   resolve calls = 2；terminal key changes = 0
///   cursor phase：0.00 → 0.25（正弦包络从中点到峰值，样式应变化）
///   snake phase：0.10 → 0.80（head_phase 差值 = 0.70，视觉应变化）
///   expected prepares = first miss(1) + animation-only hit(0) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用固定 terminal key resolve 首帧 marker 31。
///   2. 独立计算两种 cursor phase 与两种工作蛇 phase 的视觉模型。
///   3. 不改 terminal key 再次 resolve，负对照闭包 marker 为 32。
///   4. 比较 cache outcome/Arc 与两个动画视觉模型。
///
/// 预期结果：
///   - 正断言：第二次 resolve 为 Hit、Arc 身份相同、prepare count 仍为 1。
///   - 正断言：cursor 样式与工作蛇模型都随 phase 变化，证明动画没有被冻结。
///   - 负断言：animation-only repaint 不得采用 marker 32 或返回 Miss。
#[test]
fn test_terminal_render_cache_ignores_cursor_and_snake_phase() {
    let theme = Theme::paper_blue();
    let mut cache = TerminalRenderCache::<usize>::new_enabled();
    let key = TerminalRenderKey::new(102, 9, terminal_render_metrics(1.0), theme, None);
    let (first, first_outcome) = cache.resolve(key, || 31);

    let cursor_start = resolve_cursor_style(
        TerminalGlassStyle::from_theme(theme),
        TerminalCursorAnimation {
            phase: 0.0,
            focused: true,
            reduce_motion: false,
            running: true,
            display_offset: 0,
        },
    );
    let cursor_peak = resolve_cursor_style(
        TerminalGlassStyle::from_theme(theme),
        TerminalCursorAnimation {
            phase: 0.25,
            focused: true,
            reduce_motion: false,
            running: true,
            display_offset: 0,
        },
    );
    let snake_start = resolve_working_visual(WorkingVisualInput {
        theme,
        stroke_color: theme.node_stroke,
        edge_highlight_color: theme.edge_highlight,
        shadow_alpha: 0x2b,
        shadow_y: 8.0,
        shadow_blur: 24.0,
        shadow_spread: 0.0,
        border_is_emphasized: true,
        fade: 1.0,
        phase: 0.10,
    })
    .working_snake
    .expect("fade=1 的工作状态必须生成 snake");
    let snake_end = resolve_working_visual(WorkingVisualInput {
        theme,
        stroke_color: theme.node_stroke,
        edge_highlight_color: theme.edge_highlight,
        shadow_alpha: 0x2b,
        shadow_y: 8.0,
        shadow_blur: 24.0,
        shadow_spread: 0.0,
        border_is_emphasized: true,
        fade: 1.0,
        phase: 0.80,
    })
    .working_snake
    .expect("fade=1 的工作状态必须生成 snake");

    let (second, second_outcome) = cache.resolve(key, || 32);

    assert_eq!(
        first_outcome,
        TerminalCacheOutcome::Miss,
        "首次 terminal frame resolve 必须为 Miss"
    );
    assert_eq!(
        second_outcome,
        TerminalCacheOutcome::Hit,
        "只改变外部动画 phase 时相同 terminal key 必须命中"
    );
    assert!(
        Arc::ptr_eq(&first, &second),
        "cursor/snake 动画变化不得替换 terminal prepared frame Arc"
    );
    assert_eq!(
        cache.prepare_count(),
        1,
        "animation-only 第二帧不得重复 prepare terminal frame"
    );
    assert_eq!(
        *second, 31,
        "animation-only cache hit 必须继续使用首次 marker 31"
    );
    assert_ne!(
        *second, 32,
        "animation-only cache hit 不得执行负对照闭包 marker 32"
    );
    assert_ne!(
        cursor_start, cursor_peak,
        "cursor phase 从 0.00 到 0.25 时独立 paint style 必须变化"
    );
    assert_ne!(
        snake_start, snake_end,
        "工作蛇 phase 从 0.10 到 0.80 时独立视觉模型必须变化"
    );
}
