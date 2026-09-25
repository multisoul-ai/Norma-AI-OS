//! 终端卡片内容渲染 + 键盘翻译。grid 快照按等宽字体逐行绘制。

use crate::canvas::text_selection::TextRange;
use crate::cards::term::paint::TerminalRenderMetrics;
use crate::cards::term::render_cache::TerminalRenderCacheHandle;
use crate::motion::InitializationPresentation;
use canvas::NodeId;
use gpui::{px, Keystroke};
use terminal::{Key, KeyInput};
use ui::Theme;

mod render;

pub(crate) use render::render_terminal_node;

pub const TERM_FONT_PX: f32 = 13.0;
pub const TERM_LINE_PX: f32 = 16.0;
pub const TERM_CELL_PX: f32 = 8.0;
pub const TERM_PADDING_PX: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TerminalContentMode {
    IntroOnly,
    Crossfade { opacity: f32 },
    ContentOnly,
}

impl TerminalContentMode {
    pub(crate) fn requires_live_view(self) -> bool {
        !matches!(self, Self::IntroOnly)
    }

    pub(crate) fn content_opacity(self) -> f32 {
        match self {
            Self::IntroOnly => 0.0,
            Self::Crossfade { opacity } => opacity,
            Self::ContentOnly => 1.0,
        }
    }
}

pub(crate) fn terminal_content_mode(
    initialization: Option<InitializationPresentation>,
) -> TerminalContentMode {
    match initialization {
        Some(presentation) if presentation.content_opacity <= f32::EPSILON => {
            TerminalContentMode::IntroOnly
        }
        Some(presentation) => TerminalContentMode::Crossfade {
            opacity: presentation.content_opacity.clamp(0.0, 1.0),
        },
        None => TerminalContentMode::ContentOnly,
    }
}

pub struct TerminalNodeRenderOptions {
    pub node_id: NodeId,
    pub terminal_render_cache: TerminalRenderCacheHandle,
    pub theme: Theme,
    pub zoom: f32,
    pub bottom_radius_px: f32,
    pub selection: Option<TextRange>,
    pub focused: bool,
    pub reduce_motion: bool,
    pub initialization: Option<InitializationPresentation>,
    pub cursor_phase: f32,
    /// Agent 卡片以任务状态覆盖 PTY 存活状态；普通 Terminal 保持 None。
    pub cursor_task_running: Option<bool>,
}

pub fn terminal_render_metrics(zoom: f32) -> TerminalRenderMetrics {
    TerminalRenderMetrics {
        font_size: px(TERM_FONT_PX * zoom),
        line_height: px(TERM_LINE_PX * zoom),
        cell_width: px(TERM_CELL_PX * zoom),
    }
}

fn terminal_surface_bg(theme: Theme) -> u32 {
    theme.term_bg
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
    let shift = ks.modifiers.shift;

    if ks.modifiers.platform {
        return None;
    }

    if let Some(key) = named_control_key(&ks.key) {
        return Some(KeyInput {
            key,
            ctrl,
            alt,
            shift,
        });
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
                    shift: false,
                });
            }
        }
    }

    let key = match ks.key.as_str() {
        "space" => Key::Char(' '),
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        _ => return None,
    };
    Some(KeyInput {
        key,
        ctrl,
        alt,
        shift,
    })
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

    /// 终端内容区使用统一深色内容井，避免壁纸色相穿透代码表面。
    ///
    /// 数据构造（含关键数值推导）：
    ///   glass_light.node_fill alpha = 0x54 / 0xff ≈ 33%
    ///   glass_light.term_bg alpha   = 0xeb / 0xff ≈ 92%
    ///   组合 alpha = 1 - (1 - 0x54/255) * (1 - 0xeb/255) ≈ 0.947 → 0xf2
    ///   旧 surface alpha = 0x20 / 0xff ≈ 12%，组合后约 74%，仍明显吸收壁纸色相。
    ///
    /// 执行过程：
    ///   1. 从 glass_light 读取 terminal_surface_bg。
    ///   2. 比较 surface 与原型深色内容井字面量。
    ///   3. 计算和统一外壳组合后的 alpha，确认壁纸不会主导代码表面。
    ///
    /// 预期结果：
    ///   - 正断言：surface 精确复用 92% term_bg，和 HTML 原型一致。
    ///   - 正断言：组合 alpha 至少为 0xf2，保证 Terminal/Agent 代码可读。
    ///   - 负断言：surface 不得回退到 0x20 的旧轻 tint。
    #[test]
    fn test_terminal_surface_bg_uses_cohesive_dark_content_well() {
        let theme = Theme::glass_light();
        let surface = terminal_surface_bg(theme);
        let composed_surface = composited_alpha(theme.node_fill, surface);

        assert_eq!(
            surface, theme.term_bg,
            "terminal surface 应精确复用统一的 92% 深色内容井 token"
        );
        assert!(
            alpha(surface) > alpha(theme.node_fill),
            "深色内容井必须比中性外壳更不透明，才能稳定代码表面"
        );
        assert!(
            composed_surface >= 0xf2,
            "透明外壳与深色内容井组合后 alpha 至少应为 0xf2，实际为 {composed_surface:#04x}"
        );
        assert_ne!(
            alpha(surface),
            0x20,
            "terminal surface 不得回退到会吸收壁纸色相的旧 0x20 轻 tint"
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
                alt: false,
                shift: false,
            }),
            "key_char='你' 应优先于物理键 key='n'"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('n'),
                ctrl: false,
                alt: false,
                shift: false,
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
                alt: false,
                shift: false,
            }),
            "Ctrl+C 应保留 ctrl=true，不受 key_char 影响"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('c'),
                ctrl: false,
                alt: false,
                shift: false,
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
                alt: false,
                shift: false,
            }),
            "Option+a 产生的合成字符 'å' 应以 alt: false 返回，OS 已将修饰编码进字符"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('å'),
                ctrl: false,
                alt: true,
                shift: false,
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
                alt: false,
                shift: false,
            }),
            "多字符 key_char 应回落到 ks.key 处理"
        );
        assert_ne!(
            result,
            Some(KeyInput {
                key: Key::Char('b'),
                ctrl: false,
                alt: false,
                shift: false,
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
                alt: false,
                shift: false,
            }),
            "Cmd+M 不应变成裸字符 m 发送到 PTY"
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/view_enter_tests.rs"]
mod enter_tests;

#[cfg(test)]
#[path = "../../../tests/unit/cards/term/render_mode_tests.rs"]
mod render_mode_tests;
