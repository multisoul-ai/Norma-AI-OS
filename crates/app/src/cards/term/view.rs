//! 终端卡片内容渲染 + 键盘翻译。grid 快照按等宽字体逐行绘制。

use crate::cards::term::paint::{render_terminal_canvas, TerminalRenderMetrics};
use gpui::{div, prelude::*, px, rgba, AnyElement, IntoElement, Keystroke};
use runtime::NodeView;
use terminal::{Key, KeyInput, TerminalStatus};
use ui::Theme;

pub const TERM_FONT_PX: f32 = 13.0;
pub const TERM_LINE_PX: f32 = 16.0;
pub const TERM_CELL_PX: f32 = 8.0;
pub const TERM_PADDING_PX: f32 = 8.0;
const TERM_SURFACE_TINT_ALPHA: u8 = 0x3d;

/// 渲染一个终端节点内容区：退出态保留最后一帧输出，并叠加退出状态。
pub fn render_terminal_node(
    view: Option<NodeView>,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    let Some(NodeView::Term { snapshot, status }) = view else {
        return div()
            .rounded_b(px(bottom_radius_px))
            .overflow_hidden()
            .child("(session gone)")
            .into_any_element();
    };

    let terminal_canvas = render_terminal_canvas(
        snapshot,
        theme,
        TerminalRenderMetrics {
            font_size: px(TERM_FONT_PX * zoom),
            line_height: px(TERM_LINE_PX * zoom),
            cell_width: px(TERM_CELL_PX * zoom),
        },
    );

    let mut terminal = div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .relative()
        .overflow_hidden()
        .rounded_b(px(bottom_radius_px))
        .bg(rgba(terminal_surface_bg(theme)))
        .text_color(rgba(theme.term_fg))
        .font_family("Menlo") // 系统等宽字体；assets/fonts 留扩展位
        .text_size(px(TERM_FONT_PX * zoom))
        .line_height(px(TERM_LINE_PX * zoom))
        .p(px(TERM_PADDING_PX))
        .child(terminal_canvas);

    if let TerminalStatus::Exited { code } = status {
        terminal = terminal.child(
            div()
                .absolute()
                .left(px(TERM_PADDING_PX))
                .right(px(TERM_PADDING_PX))
                .bottom(px(TERM_PADDING_PX))
                .bg(rgba(terminal_surface_bg(theme)))
                .text_color(rgba(theme.term_exit_fg))
                .child(process_exit_label(code)),
        );
    }

    terminal.into_any_element()
}

fn terminal_surface_bg(theme: Theme) -> u32 {
    with_alpha(theme.term_bg, TERM_SURFACE_TINT_ALPHA)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
fn alpha(color: u32) -> u8 {
    (color & 0xff) as u8
}

#[cfg(test)]
fn composited_alpha(bottom: u32, top: u32) -> u8 {
    let bottom = f32::from(alpha(bottom)) / 255.0;
    let top = f32::from(alpha(top)) / 255.0;
    ((1.0 - (1.0 - bottom) * (1.0 - top)) * 255.0).round() as u8
}

fn process_exit_label(code: Option<i32>) -> String {
    format!(
        "[process exited{}]",
        match code {
            Some(c) => format!(" code={c}"),
            None => String::new(),
        }
    )
}

/// gpui 按键 → 终端 KeyInput。无法映射的返回 None。
pub fn keystroke_to_input(ks: &Keystroke) -> Option<KeyInput> {
    let ctrl = ks.modifiers.control;
    let alt = ks.modifiers.alt;

    // ks.key 是逻辑键名字符串，如 "a"、"enter"、"escape"、"up"、"tab"、"backspace"、"space"。
    let key = match ks.key.as_str() {
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "escape" => Key::Escape,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "space" => Key::Char(' '),
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        _ => return None,
    };
    Some(KeyInput { key, ctrl, alt })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 退出状态标签保留退出码，供终端输出旁边显示。
    ///
    /// 数据构造（含关键数值推导）：
    ///   code=None    → 无 OS 退出码，标签不追加 code
    ///   code=Some(1) → 子进程错误退出，标签追加 " code=1"
    ///
    /// 执行过程：
    ///   1. process_exit_label(None)
    ///   2. process_exit_label(Some(1))
    ///
    /// 预期结果：
    ///   - 正断言：None 显示 "[process exited]"
    ///   - 正断言：Some(1) 显示 "[process exited code=1]"
    ///   - 负断言：None 不应包含 code=1
    #[test]
    fn test_process_exit_label_includes_exit_code_when_available() {
        let no_code = process_exit_label(None);
        let with_code = process_exit_label(Some(1));

        assert_eq!(
            no_code, "[process exited]",
            "无退出码时标签应只显示进程已退出"
        );
        assert_eq!(
            with_code, "[process exited code=1]",
            "有退出码时标签应包含具体 code=1"
        );
        assert!(!no_code.contains("code=1"), "无退出码标签不应误显示 code=1");
    }

    /// 终端内容区使用轻量 tint，而不是再次铺满高不透明 term_bg。
    ///
    /// 数据构造（含关键数值推导）：
    ///   glass_dark.node_fill alpha = 0xd9 / 0xff ≈ 85%
    ///   glass_dark.term_bg alpha   = 0xfa / 0xff ≈ 98%
    ///   terminal_surface_bg alpha  = 0x3d / 0xff ≈ 24%
    ///   组合 alpha = 1 - (1 - 0xd9/255) * (1 - 0x3d/255) ≈ 0.887 → 0xe2
    ///   旧组合 alpha = 1 - (1 - 0xd9/255) * (1 - 0xfa/255) ≈ 0.997 → 0xfe
    ///
    /// 执行过程：
    ///   1. 从 glass_dark 派生 terminal_surface_bg
    ///   2. 比较 surface alpha、原始 term_bg alpha、与 node_fill 组合后的 alpha
    ///   3. 对比旧的 node_fill + term_bg 组合，确认不再接近实心
    ///
    /// 预期结果：
    ///   - 正断言：surface alpha 为 0x3d，明显低于 term_bg
    ///   - 正断言：node_fill + surface 组合 alpha 不超过 0xe6，保留玻璃感
    ///   - 负断言：node_fill + surface 不应等于旧的 node_fill + term_bg 近实心组合
    #[test]
    fn test_terminal_surface_bg_keeps_liquid_glass_transparency() {
        let theme = Theme::glass_dark();
        let surface = terminal_surface_bg(theme);
        let composed_surface = composited_alpha(theme.node_fill, surface);
        let composed_old = composited_alpha(theme.node_fill, theme.term_bg);

        assert_eq!(
            alpha(surface),
            TERM_SURFACE_TINT_ALPHA,
            "terminal surface 应使用轻量 tint alpha 0x3d，而不是原始 term_bg alpha"
        );
        assert!(
            alpha(surface) < alpha(theme.node_fill),
            "terminal surface alpha 应低于 node_fill，避免内容区比玻璃外壳更实"
        );
        assert!(
            alpha(surface) < alpha(theme.term_bg),
            "terminal surface alpha 应明显低于原始 term_bg，否则仍会像黑色实心块"
        );
        assert!(
            composed_surface <= 0xe6,
            "node_fill + terminal surface 组合 alpha 应保持在玻璃范围内，实际为 {composed_surface:#04x}"
        );
        assert_ne!(
            composed_surface, composed_old,
            "terminal surface 不应退化为旧的 node_fill + term_bg 近实心组合"
        );
    }
}
