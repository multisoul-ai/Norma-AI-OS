use super::*;
use crate::cards::term::cursor::TerminalCursorAnimation;
use crate::cards::term::paint::{render_terminal_canvas, TerminalCanvasOptions};
use crate::cards::term::render_cache::new_terminal_render_cache_handle_enabled;
use gpui::{point, px, size, IntoElement, TestAppContext};
use std::sync::Arc;
use terminal::{Cell, CellAttrs, GridSnapshot, TerminalColor, TerminalNamedColor};

fn terminal_cell(c: char) -> Cell {
    Cell {
        c,
        fg: TerminalColor::Named(TerminalNamedColor::Foreground),
        bg: TerminalColor::Named(TerminalNamedColor::Background),
        attrs: CellAttrs::default(),
        hyperlink: None,
    }
}

fn terminal_row(text: &str, cols: usize) -> Vec<Cell> {
    text.chars()
        .chain(std::iter::repeat(' '))
        .take(cols)
        .map(terminal_cell)
        .collect()
}

fn two_run_snapshot() -> GridSnapshot {
    let cols = 8;
    GridSnapshot {
        rows: vec![terminal_row("AB", cols), terminal_row("CD", cols)],
        cursor_line: 1,
        cursor_col: 2,
        cols,
        lines: 2,
        display_offset: 0,
        history_size: 0,
        alt_screen: false,
        alternate_scroll: true,
        mouse_mode: false,
        sgr_mouse: false,
        show_cursor: true,
    }
}

/// TRC-7：三次 animation-only GPUI 绘制必须只为两条 terminal text run shaping 一次。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot rows = ["AB", "CD"]，每行相同样式连续字符合并为 1 run
///   visible text runs = row0(1) + row1(1) = 2
///   terminal node_id = 107；revision = 13；zoom = 1.0；selection=None
///   animation-only draws = 3；cursor phase = 0.00 / 0.25 / 0.75
///   expected frame prepares = first miss(1) + two hits(0) = 1
///   expected shape prepares = runs(2) × cache misses(1) = 2，而不是 2 × 3 = 6
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实 GPUI TestAppContext/Window 与共享 TerminalRenderCache handle。
///   2. 第一次 draw：prepaint 构建 layout 并 shape "AB"/"CD"，paint 提交两条文字。
///   3. 保持 node/revision/metrics/theme/selection 不变，只改 cursor phase 再 draw 两次。
///   4. 从 cache 读取 prepared frame 身份、文字位置与 prepare 诊断。
///
/// 预期结果：
///   - 正断言：三次 draw 后 frame prepare=1、shape prepare=2、缓存仍含两条 shaped run。
///   - 正断言：两条 run 文本分别为 AB/CD，位置为 row0/row1、start_col=0。
///   - 负断言：shape prepare 不得为 6，prepared frame Arc 不得被 animation-only draw 替换。
#[test]
fn test_terminal_prepared_frame_shapes_text_once_across_animation_only_repaints() {
    let cache = new_terminal_render_cache_handle_enabled();
    let snapshot = Arc::new(two_run_snapshot());
    let theme = Theme::paper_blue();
    let metrics = terminal_render_metrics(1.0);
    let mut gpui = TestAppContext::single();
    let visual = gpui.add_empty_window();
    let mut prepared_frames = Vec::new();

    for phase in [0.0, 0.25, 0.75] {
        let cache_for_draw = cache.clone();
        let snapshot_for_draw = Arc::clone(&snapshot);
        visual.draw(
            point(px(0.0), px(0.0)),
            size(px(320.0), px(80.0)),
            move |_window, _cx| {
                render_terminal_canvas(TerminalCanvasOptions {
                    node_id: 107,
                    snapshot: snapshot_for_draw,
                    revision: 13,
                    cache: cache_for_draw,
                    theme,
                    metrics,
                    selection: None,
                    cursor_animation: TerminalCursorAnimation {
                        phase,
                        focused: true,
                        reduce_motion: false,
                        running: true,
                        display_offset: 0,
                    },
                })
                .into_any_element()
            },
        );
        prepared_frames.push(
            cache
                .borrow()
                .entry_for_test(107)
                .expect("每次 draw 后 node 107 都必须有 prepared frame"),
        );
    }

    let cache_ref = cache.borrow();
    let final_frame = prepared_frames
        .last()
        .expect("三次 draw 应记录三份 prepared frame 引用");

    assert_eq!(
        cache_ref.prepare_count(),
        1,
        "三次 animation-only draw 只允许首次 prepare terminal frame"
    );
    assert_eq!(
        cache_ref.prepared_item_count(),
        2,
        "两条 text run 应各 shape 一次，总 shape prepare count 必须为 2"
    );
    assert_ne!(
        cache_ref.prepared_item_count(),
        6,
        "shape prepare 不得按 2 runs × 3 animation draws 重复到 6 次"
    );
    assert!(
        Arc::ptr_eq(&prepared_frames[0], &prepared_frames[1]),
        "第一次与第二次 animation-only draw 必须复用 prepared frame Arc"
    );
    assert!(
        Arc::ptr_eq(&prepared_frames[1], &prepared_frames[2]),
        "第二次与第三次 animation-only draw 必须复用 prepared frame Arc"
    );
    assert_eq!(
        final_frame.shaped_text_runs.len(),
        2,
        "prepared frame 必须保留可绘制的两条 shaped text run"
    );
    assert_eq!(
        final_frame.shaped_text_runs[0].shaped.text.as_ref(),
        "AB",
        "第一条 shaped run 文本必须为 AB"
    );
    assert_eq!(
        (
            final_frame.shaped_text_runs[0].row,
            final_frame.shaped_text_runs[0].start_col
        ),
        (0, 0),
        "第一条 shaped run 必须位于 row0/start_col0"
    );
    assert_eq!(
        final_frame.shaped_text_runs[1].shaped.text.as_ref(),
        "CD",
        "第二条 shaped run 文本必须为 CD"
    );
    assert_eq!(
        (
            final_frame.shaped_text_runs[1].row,
            final_frame.shaped_text_runs[1].start_col
        ),
        (1, 0),
        "第二条 shaped run 必须位于 row1/start_col0"
    );
    assert_ne!(
        final_frame.shaped_text_runs[1].row, final_frame.shaped_text_runs[0].row,
        "第二条 run 不得错误复用第一条 run 的 row0 位置"
    );
}
