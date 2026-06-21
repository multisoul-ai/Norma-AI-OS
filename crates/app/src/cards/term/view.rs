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

    if ks.modifiers.platform {
        return None;
    }

    if let Some(key) = named_control_key(&ks.key) {
        return Some(KeyInput { key, ctrl, alt });
    }

    // When key_char holds exactly one char and no Ctrl/Cmd is active, use it
    // directly — this is how IME-composed characters (Chinese, Japanese, etc.)
    // and non-ASCII layouts reach us from GPUI.
    if !ctrl {
        if let Some(kc) = &ks.key_char {
            let mut chars = kc.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                return Some(KeyInput {
                    key: Key::Char(c),
                    ctrl: false,
                    alt: false,
                });
            }
        }
    }

    let key = match ks.key.as_str() {
        "space" => Key::Char(' '),
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        _ => return None,
    };
    Some(KeyInput { key, ctrl, alt })
}

fn named_control_key(key: &str) -> Option<Key> {
    match key {
        "enter" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "backspace" => Some(Key::Backspace),
        "escape" => Some(Key::Escape),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        _ => None,
    }
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

    /// key_char 优先于 key：IME 上屏或非 ASCII 布局键按下时，key_char 携带真实字符。
    ///
    /// 数据构造：
    ///   ks.key = "n"（拼音字母，物理键）
    ///   ks.key_char = Some("你")（IME 确认上屏的汉字）
    ///   modifiers: 无 ctrl/alt/platform
    ///
    /// 执行过程：
    ///   1. keystroke_to_input(&ks)
    ///   2. key_char 为 Some("你")，单字符，无修饰 → 应取 '你' 作为 Key::Char
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(KeyInput { key: Key::Char('你'), ctrl: false, alt: false })
    ///   - 负断言：不应返回 Key::Char('n')（物理键）
    #[test]
    fn test_keystroke_to_input_prefers_key_char_for_ime() {
        use gpui::{Keystroke, Modifiers};
        let ks = Keystroke {
            key: "n".into(),
            key_char: Some("你".into()),
            modifiers: Modifiers::default(),
        };
        let result = keystroke_to_input(&ks);
        assert_eq!(
            result,
            Some(KeyInput {
                key: Key::Char('你'),
                ctrl: false,
                alt: false
            }),
            "key_char='你' 应优先于物理键 key='n'"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('n'),
                ctrl: false,
                alt: false
            }),
            "不应返回物理键字符 'n'，key_char 优先级更高"
        );
    }

    /// Ctrl 修饰时不使用 key_char：Ctrl+C 应编码为控制码，而不是 key_char 中可能含有的字符。
    ///
    /// 数据构造：
    ///   ks.key = "c"
    ///   ks.key_char = Some("c")（macOS 可能仍然填充 key_char）
    ///   modifiers: ctrl = true
    ///
    /// 执行过程：
    ///   1. keystroke_to_input(&ks)
    ///   2. ctrl=true → 走 key 路径，Key::Char('c') with ctrl=true
    ///
    /// 预期结果：
    ///   - 正断言：ctrl=true，key=Char('c')
    ///   - 负断言：返回值不为 ctrl=false（key_char 路径未被走）
    #[test]
    fn test_keystroke_to_input_ctrl_ignores_key_char() {
        use gpui::{Keystroke, Modifiers};
        let ks = Keystroke {
            key: "c".into(),
            key_char: Some("c".into()),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };
        let result = keystroke_to_input(&ks);
        assert_eq!(
            result,
            Some(KeyInput {
                key: Key::Char('c'),
                ctrl: true,
                alt: false
            }),
            "Ctrl+C 应保留 ctrl=true，不受 key_char 影响"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('c'),
                ctrl: false,
                alt: false
            }),
            "ctrl 修饰时不应跳过 ctrl=true 标志走 key_char 路径"
        );
    }

    /// 场景：Alt+a 在 macOS 上由 OS 合成 "å"，key_char 路径应剥离 alt 标志，避免 encode() 再次前缀 ESC。
    ///
    /// 数据构造：
    ///   ks.key = "a"（物理按键）
    ///   ks.key_char = Some("å")（macOS Option 键合成结果）
    ///   modifiers: alt = true，ctrl = false，platform = false
    ///   编码侧：encode() 对 alt=true 的 KeyInput 会前缀 ESC(0x1b)，
    ///           若 alt 未剥离则输出 ESC+'å' 而非单字符 'å'
    ///
    /// 执行过程：
    ///   1. keystroke_to_input(&ks) → 进入 key_char 守卫（无 ctrl/platform）
    ///   2. key_char = "å"，单字符 → 命中守卫，返回 KeyInput { key: Char('å'), ctrl: false, alt: false }
    ///   3. alt 修饰在合成字符里已体现，故置 alt: false 避免双重应用
    ///
    /// 预期结果：
    ///   - 正断言：返回 Some(KeyInput { key: Key::Char('å'), ctrl: false, alt: false })
    ///   - 负断言：不应返回 alt: true（否则 encode() 会多插入一个 ESC）
    #[test]
    fn test_keystroke_to_input_alt_key_char_strips_alt_flag() {
        use gpui::{Keystroke, Modifiers};
        let ks = Keystroke {
            key: "a".into(),
            key_char: Some("å".into()),
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        };
        let result = keystroke_to_input(&ks);
        assert_eq!(
            result,
            Some(KeyInput {
                key: Key::Char('å'),
                ctrl: false,
                alt: false
            }),
            "Option+a 产生的合成字符 'å' 应以 alt: false 返回，OS 已将修饰编码进字符"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('å'),
                ctrl: false,
                alt: true
            }),
            "alt: true 会导致 encode() 在 'å' 前多插 ESC，modifier 不应被双重应用"
        );
    }

    /// key_char 多字符时回落 key 路径（例如组字中间态，不应上屏多个字符）。
    ///
    /// 数据构造：
    ///   ks.key = "a"
    ///   ks.key_char = Some("ab")（不合法的多字符 key_char，fallback 到 key）
    ///   modifiers: 无
    ///
    /// 执行过程：
    ///   1. keystroke_to_input(&ks)
    ///   2. key_char.chars().count() > 1 → 回落，使用 ks.key = "a"
    ///
    /// 预期结果：
    ///   - 正断言：返回 Key::Char('a')
    ///   - 负断言：多字符 key_char 不应取第二个字符
    #[test]
    fn test_keystroke_to_input_multi_char_key_char_falls_back_to_key() {
        use gpui::{Keystroke, Modifiers};
        let ks = Keystroke {
            key: "a".into(),
            key_char: Some("ab".into()),
            modifiers: Modifiers::default(),
        };
        let result = keystroke_to_input(&ks);
        assert_eq!(
            result,
            Some(KeyInput {
                key: Key::Char('a'),
                ctrl: false,
                alt: false
            }),
            "多字符 key_char 应回落到 ks.key 处理"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('b'),
                ctrl: false,
                alt: false
            }),
            "多字符 key_char 不应取第二个字符"
        );
    }

    /// 平台快捷键不透传 PTY：Cmd+M 应由 app action 处理，而不是作为字符 m 发给终端。
    ///
    /// 数据构造（含关键数值推导）：
    ///   ks.key = "m"
    ///   modifiers.platform = true（macOS Command 键）
    ///   key_char = Some("m")（系统可能仍提供字符，但平台快捷键优先级更高）
    ///
    /// 执行过程：
    ///   1. keystroke_to_input(&ks)
    ///   2. platform=true → 返回 None，让 GPUI action/快捷键处理
    ///
    /// 预期结果：
    ///   - 正断言：返回 None
    ///   - 负断言：不应返回 Key::Char('m')，否则 Cmd+M 会输入到 PTY
    #[test]
    fn test_keystroke_to_input_platform_shortcut_is_not_sent_to_pty() {
        use gpui::{Keystroke, Modifiers};
        let ks = Keystroke {
            key: "m".into(),
            key_char: Some("m".into()),
            modifiers: Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        };

        let result = keystroke_to_input(&ks);

        assert_eq!(
            result, None,
            "Cmd+M 应交给 app action 处理，不应生成 terminal KeyInput"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('m'),
                ctrl: false,
                alt: false
            }),
            "Cmd+M 不应变成裸字符 m 发送到 PTY"
        );
    }
}

#[cfg(test)]
#[path = "view_enter_tests.rs"]
mod enter_tests;
