//! 终端节点渲染 + 键盘翻译。grid 快照按等宽字体逐行绘制。

use crate::app::MultisoulApp;
use crate::terminal_renderer::{render_terminal_canvas, TerminalRenderMetrics};
use gpui::{div, prelude::*, px, rgba, Context, Keystroke, Window};
use terminal::{Key, KeyInput, TerminalSessionId, TerminalStatus};

pub const TERM_FONT_PX: f32 = 13.0;
pub const TERM_LINE_PX: f32 = 16.0;
pub const TERM_CELL_PX: f32 = 8.0;
pub const TERM_PADDING_PX: f32 = 8.0;

/// 渲染一个终端节点内容区：退出态保留最后一帧输出，并叠加退出状态。
pub fn render_terminal_node(
    app: &mut MultisoulApp,
    session_id: TerminalSessionId,
    zoom: f32,
    bottom_radius_px: f32,
    _window: &mut Window,
    _cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let theme = app.theme;
    let Some(session) = app.runtime.session(session_id) else {
        return div()
            .rounded_b(px(bottom_radius_px))
            .overflow_hidden()
            .child("(session gone)");
    };

    let snap = session.snapshot();
    let terminal_canvas = render_terminal_canvas(
        snap,
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
        .bg(rgba(theme.term_bg))
        .text_color(rgba(theme.term_fg))
        .font_family("Menlo") // 系统等宽字体；assets/fonts 留扩展位
        .text_size(px(TERM_FONT_PX * zoom))
        .line_height(px(TERM_LINE_PX * zoom))
        .p(px(TERM_PADDING_PX))
        .child(terminal_canvas);

    if let TerminalStatus::Exited { code } = session.status {
        terminal = terminal.child(
            div()
                .absolute()
                .left(px(TERM_PADDING_PX))
                .right(px(TERM_PADDING_PX))
                .bottom(px(TERM_PADDING_PX))
                .bg(rgba(theme.term_bg))
                .text_color(rgba(theme.term_exit_fg))
                .child(process_exit_label(code)),
        );
    }

    terminal
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
}
