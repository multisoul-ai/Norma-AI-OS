use super::*;
use ui::Theme;

fn test_glass_style() -> TerminalGlassStyle {
    TerminalGlassStyle::from_theme(Theme::glass_dark())
}

fn animated_cursor(phase: f32) -> TerminalCursorAnimation {
    TerminalCursorAnimation {
        phase,
        focused: true,
        reduce_motion: false,
        running: true,
        display_offset: 0,
    }
}

/// 聚焦且运行中的 terminal cursor 应随 phase 呼吸改变透明度。
///
/// 数据构造（含关键数值推导）：
///   phase=0.25 → sin(0.25*2π)=1，envelope=1，高亮态
///   phase=0.75 → sin(0.75*2π)=-1，envelope=0，暗态
///   focused=true、running=true、reduce_motion=false、display_offset=0 → 动画启用
///
/// 执行过程：
///   1. 用 phase=0.25 生成 bright cursor style
///   2. 用 phase=0.75 生成 dim cursor style
///   3. 比较 fill/stroke/highlight alpha
///
/// 预期结果：
///   - 正断言：bright fill alpha 大于 dim fill alpha
///   - 正断言：bright stroke alpha 大于 dim stroke alpha
///   - 正断言：bright highlight alpha 大于 dim highlight alpha
///   - 负断言：bright 与 dim 的 fill alpha 不应相等，否则 phase 未参与绘制
#[test]
fn test_focused_running_cursor_breathes_alpha_by_phase() {
    let bright = resolve_cursor_style(test_glass_style(), animated_cursor(0.25));
    let dim = resolve_cursor_style(test_glass_style(), animated_cursor(0.75));

    assert!(
        bright.fill.a > dim.fill.a,
        "phase=0.25 的 cursor fill alpha 应高于 phase=0.75"
    );
    assert!(
        bright.stroke.a > dim.stroke.a,
        "phase=0.25 的 cursor stroke alpha 应高于 phase=0.75"
    );
    assert!(
        bright.highlight.a > dim.highlight.a,
        "phase=0.25 的 cursor highlight alpha 应高于 phase=0.75"
    );
    assert_ne!(
        bright.fill.a, dim.fill.a,
        "cursor fill alpha 不应在不同 phase 下相等，否则没有呼吸动画"
    );
}

/// Reduce Motion 开启时 terminal cursor 应保持静态，不随 phase 改变。
///
/// 数据构造（含关键数值推导）：
///   phase=0.25 → 动画峰值相位
///   phase=0.75 → 动画谷值相位
///   reduce_motion=true → 动画被关闭，两个 phase 应得到同一静态模型
///
/// 执行过程：
///   1. 分别用 phase=0.25 和 phase=0.75 生成 cursor style
///   2. 比较三个 alpha 通道
///
/// 预期结果：
///   - 正断言：fill alpha 相等
///   - 正断言：stroke alpha 相等
///   - 正断言：highlight alpha 相等
///   - 负断言：reduce motion 下 fill alpha 不应等于完全不透明
#[test]
fn test_reduce_motion_keeps_cursor_alpha_static_across_phase() {
    let first = resolve_cursor_style(
        test_glass_style(),
        TerminalCursorAnimation {
            reduce_motion: true,
            ..animated_cursor(0.25)
        },
    );
    let second = resolve_cursor_style(
        test_glass_style(),
        TerminalCursorAnimation {
            reduce_motion: true,
            ..animated_cursor(0.75)
        },
    );

    assert_eq!(
        first.fill.a, second.fill.a,
        "reduce motion 下 cursor fill alpha 不应随 phase 改变"
    );
    assert_eq!(
        first.stroke.a, second.stroke.a,
        "reduce motion 下 cursor stroke alpha 不应随 phase 改变"
    );
    assert_eq!(
        first.highlight.a, second.highlight.a,
        "reduce motion 下 cursor highlight alpha 不应随 phase 改变"
    );
    assert_ne!(
        first.fill.a, 1.0,
        "reduce motion 下 cursor fill 仍应保持玻璃半透明，不应变成实心块"
    );
}

/// 查看 scrollback 时 terminal cursor 应降级为静态淡态，不随 phase 强呼吸。
///
/// 数据构造（含关键数值推导）：
///   display_offset=3 → 用户正在看历史输出，不在底部实时屏幕
///   phase=0.25 与 phase=0.75 是呼吸峰谷；scrollback 下应忽略两者差异
///
/// 执行过程：
///   1. 用 display_offset=3、phase=0.25 生成 first style
///   2. 用 display_offset=3、phase=0.75 生成 second style
///   3. 比较 fill/stroke/highlight alpha
///
/// 预期结果：
///   - 正断言：fill alpha 相等
///   - 正断言：stroke alpha 相等
///   - 正断言：highlight alpha 相等
///   - 负断言：scrollback 下 fill alpha 不应等于动画峰值 0.52
#[test]
fn test_scrollback_cursor_uses_static_dim_alpha() {
    let first = resolve_cursor_style(
        test_glass_style(),
        TerminalCursorAnimation {
            display_offset: 3,
            ..animated_cursor(0.25)
        },
    );
    let second = resolve_cursor_style(
        test_glass_style(),
        TerminalCursorAnimation {
            display_offset: 3,
            ..animated_cursor(0.75)
        },
    );

    assert_eq!(
        first.fill.a, second.fill.a,
        "scrollback 下 cursor fill alpha 不应随 phase 改变"
    );
    assert_eq!(
        first.stroke.a, second.stroke.a,
        "scrollback 下 cursor stroke alpha 不应随 phase 改变"
    );
    assert_eq!(
        first.highlight.a, second.highlight.a,
        "scrollback 下 cursor highlight alpha 不应随 phase 改变"
    );
    assert_ne!(
        first.fill.a, 0.52,
        "scrollback 下 cursor fill 不应使用动画峰值，否则历史内容会出现强动效"
    );
}

/// terminal cursor phase 按 1200ms 周期前进并回绕。
///
/// 数据构造（含关键数值推导）：
///   周期 = 1200ms
///   phase=0.0 + 600ms / 1200ms = 0.5
///   phase=0.75 + 600ms / 1200ms = 1.25 → rem_euclid(1.0)=0.25
///
/// 执行过程：
///   1. advance_terminal_cursor_phase(0.0, 600ms)
///   2. advance_terminal_cursor_phase(0.75, 600ms)
///   3. 用 epsilon 比较浮点结果
///
/// 预期结果：
///   - 正断言：半周期后 phase 为 0.5
///   - 正断言：跨周期后 phase 回绕为 0.25
///   - 负断言：跨周期后 phase 不应大于等于 1.0
#[test]
fn test_advance_terminal_cursor_phase_wraps_at_period() {
    let half = advance_terminal_cursor_phase(0.0, Duration::from_millis(600));
    let wrapped = advance_terminal_cursor_phase(0.75, Duration::from_millis(600));

    assert!(
        (half - 0.5).abs() < 0.0001,
        "600ms 是 1200ms 周期的一半，phase 应前进到 0.5"
    );
    assert!(
        (wrapped - 0.25).abs() < 0.0001,
        "0.75 加半周期应回绕到 0.25"
    );
    assert!(
        wrapped < 1.0,
        "phase 回绕后不应大于等于 1.0，否则后续 envelope 会不稳定"
    );
}
