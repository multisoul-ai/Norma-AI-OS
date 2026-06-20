//! 终端节点渲染 + 键盘翻译。grid 快照按等宽字体逐行绘制。

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, Context, Keystroke, Window};
use terminal::{Key, KeyInput, TerminalSessionId, TerminalStatus};

const FONT_PX: f32 = 13.0;
const LINE_PX: f32 = 16.0;

/// 渲染一个终端节点内容区：退出态显示占位；否则逐行画 grid。
pub fn render_terminal_node(
    app: &mut MultisoulApp,
    session_id: TerminalSessionId,
    zoom: f32,
    _window: &mut Window,
    _cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let theme = app.theme;
    let Some(session) = app.runtime.session(session_id) else {
        return div().child("(session gone)");
    };

    // SPEC §8：进程退出 → 显示已退出占位，不崩溃。
    if let TerminalStatus::Exited { code } = session.status {
        return div()
            .size_full()
            .bg(gpui::rgb(theme.term_bg))
            .text_color(gpui::rgb(0xff6b6b))
            .p(px(8.0))
            .child(format!(
                "[process exited{}]",
                match code {
                    Some(c) => format!(" code={c}"),
                    None => String::new(),
                }
            ));
    }

    let snap = session.snapshot();
    let mut lines = Vec::with_capacity(snap.lines);
    for row in &snap.rows {
        let text: String = row.iter().map(|c| c.c).collect();
        lines.push(div().h(px(LINE_PX * zoom)).child(text));
    }

    div()
        .size_full()
        .bg(gpui::rgb(theme.term_bg))
        .text_color(gpui::rgb(theme.term_fg))
        .font_family("Menlo") // 系统等宽字体；assets/fonts 留扩展位
        .text_size(px(FONT_PX * zoom))
        .p(px(4.0))
        .children(lines)
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
