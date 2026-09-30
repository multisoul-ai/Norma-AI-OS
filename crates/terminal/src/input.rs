//! 按键 → 写入 PTY 的字节序列编码（零 gpui）。
//! app 层负责把 gpui 键盘事件翻译成本模块的 KeyInput。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInput {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// 把一次按键编码为写入 PTY 的字节序列。
pub fn encode(input: &KeyInput) -> Vec<u8> {
    match input.key {
        Key::Char(c) => {
            // Ctrl + 字母 → control code（c & 0x1f）。仅对 ASCII 字母有意义。
            if input.ctrl && c.is_ascii_alphabetic() {
                return vec![(c.to_ascii_lowercase() as u8) & 0x1f];
            }
            let mut bytes = Vec::new();
            if input.alt {
                bytes.push(0x1b); // meta = ESC 前缀
            }
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            bytes
        }
        Key::Enter => vec![0x0d],     // CR
        Key::Tab => vec![0x09],       // HT
        Key::Backspace => vec![0x7f], // DEL
        Key::Escape => vec![0x1b],    // ESC
        Key::Up => encode_arrow(input, b'A'),
        Key::Down => encode_arrow(input, b'B'),
        Key::Right => encode_arrow(input, b'C'),
        Key::Left => encode_arrow(input, b'D'),
    }
}

fn encode_arrow(input: &KeyInput, direction: u8) -> Vec<u8> {
    // xterm: modifier = 1 + Shift(1) + Alt(2) + Ctrl(4)。无修饰键沿用短序列。
    let modifier = 1 + u8::from(input.shift) + 2 * u8::from(input.alt) + 4 * u8::from(input.ctrl);
    if modifier == 1 {
        vec![0x1b, b'[', direction]
    } else {
        vec![0x1b, b'[', b'1', b';', b'0' + modifier, direction]
    }
}

/// 按终端应用声明的增强键盘模式编码按键。
///
/// Option+Enter / Shift+Enter 在 legacy 模式下与 Enter 都是 CR；应用请求 Kitty 键盘增强
/// 协议后，才能用 CSI-u 保留修饰键，避免多行编辑被误识别为提交。
pub(crate) fn encode_for_keyboard_mode(input: &KeyInput, enhanced_keyboard: bool) -> Vec<u8> {
    if enhanced_keyboard && matches!(input.key, Key::Enter) && !input.ctrl {
        return match (input.shift, input.alt) {
            (true, false) => b"\x1b[13;2u".to_vec(),
            (false, true) => b"\x1b[13;3u".to_vec(),
            (true, true) => b"\x1b[13;4u".to_vec(),
            (false, false) => encode(input),
        };
    }
    encode(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ctrl+C 编码为 ETX(0x03)。
    ///
    /// 数值推导：control code = ascii(letter) & 0x1f
    ///   'c' = 0x63；0x63 & 0x1f = 0x03（ETX，即 SIGINT 触发字节）
    ///
    /// 预期结果：
    ///   - 正断言：encode(Ctrl+'c') == [0x03]
    ///   - 负断言：!= [0x63]（不是裸字符 'c'，证明 ctrl 生效）
    #[test]
    fn test_ctrl_c_encodes_to_etx() {
        let out = encode(&KeyInput {
            key: Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        });
        assert_eq!(out, vec![0x03], "Ctrl+C 应编码为 0x03（'c'&0x1f）");
        assert_ne!(out, vec![0x63], "不应是裸 'c'(0x63)，说明 ctrl 修饰未生效");
    }

    /// 普通可见字符按 UTF-8 编码。
    ///
    /// 预期结果：
    ///   - 正断言：'a' → [0x61]
    #[test]
    fn test_plain_char() {
        let out = encode(&KeyInput {
            key: Key::Char('a'),
            ctrl: false,
            alt: false,
            shift: false,
        });
        assert_eq!(out, vec![0x61], "'a' 应编码为单字节 0x61");
    }

    /// Enter / Tab / Backspace / Escape 的控制字节。
    ///
    /// 数值推导：Enter=CR(0x0d), Tab=HT(0x09), Backspace=DEL(0x7f), Escape=ESC(0x1b)
    ///
    /// 预期结果：四个特殊键各自映射正确
    #[test]
    fn test_special_control_bytes() {
        assert_eq!(
            encode(&KeyInput {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x0d],
            "Enter → CR"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Tab,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x09],
            "Tab → HT"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Backspace,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x7f],
            "Backspace → DEL"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Escape,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x1b],
            "Escape → ESC"
        );
    }

    /// 方向键编码为 CSI 序列 ESC [ A/B/C/D。
    ///
    /// 数值推导：Up = ESC '[' 'A' = [0x1b, 0x5b, 0x41]
    ///
    /// 预期结果：
    ///   - 正断言：Up → [0x1b, b'[', b'A']
    ///   - 负断言：Up 序列长度为 3（不是裸字节）
    #[test]
    fn test_arrow_keys_csi() {
        let up = encode(&KeyInput {
            key: Key::Up,
            ctrl: false,
            alt: false,
            shift: false,
        });
        assert_eq!(up, vec![0x1b, b'[', b'A'], "Up 应为 CSI A 序列");
        assert_eq!(up.len(), 3, "方向键应是 3 字节 CSI 序列");
        assert_eq!(
            encode(&KeyInput {
                key: Key::Down,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x1b, b'[', b'B'],
            "Down → CSI B"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Right,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x1b, b'[', b'C'],
            "Right → CSI C"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Left,
                ctrl: false,
                alt: false,
                shift: false,
            }),
            vec![0x1b, b'[', b'D'],
            "Left → CSI D"
        );
    }

    /// 修饰方向键使用 xterm CSI 1;modifier 序列，保留 TUI 快捷键语义。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   modifier = 1(default) + 1(shift) + 2(alt) + 4(ctrl)。
    ///   Option+Up = 1+2=3 → ESC[1;3A；Ctrl+Right = 1+4=5 → ESC[1;5C。
    ///   Shift+Down = 1+1=2 → ESC[1;2B；Shift+Option+Ctrl+Left = 1+1+2+4=8 → ESC[1;8D。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 分别编码四种修饰方向键。
    ///   2. 对照各自应发送给 PTY 的完整字节序列。
    ///
    /// 预期结果：
    ///   - 正断言：四个组合均保留方向和修饰键。
    ///   - 负断言：Option+Up 不得退化为普通 Up。
    #[test]
    fn test_modified_arrow_keys_keep_xterm_modifiers() {
        let option_up = encode(&KeyInput {
            key: Key::Up,
            ctrl: false,
            alt: true,
            shift: false,
        });
        let ctrl_right = encode(&KeyInput {
            key: Key::Right,
            ctrl: true,
            alt: false,
            shift: false,
        });
        let shift_down = encode(&KeyInput {
            key: Key::Down,
            ctrl: false,
            alt: false,
            shift: true,
        });
        let all_left = encode(&KeyInput {
            key: Key::Left,
            ctrl: true,
            alt: true,
            shift: true,
        });

        assert_eq!(option_up, b"\x1b[1;3A", "Option+Up 应保留 Alt 修饰值 3");
        assert_eq!(ctrl_right, b"\x1b[1;5C", "Ctrl+Right 应保留 Ctrl 修饰值 5");
        assert_eq!(shift_down, b"\x1b[1;2B", "Shift+Down 应保留 Shift 修饰值 2");
        assert_eq!(all_left, b"\x1b[1;8D", "三个修饰键同时按下应得到值 8");
        assert_ne!(option_up, b"\x1b[A", "Option+Up 不能退化为普通 Up");
    }

    /// Alt+char 前缀 ESC（meta 键约定：ESC 后跟字符）。
    ///
    /// 数值推导：Alt+'b' = ESC 'b' = [0x1b, 0x62]
    ///
    /// 预期结果：
    ///   - 正断言：Alt+'b' → [0x1b, 0x62]
    #[test]
    fn test_alt_prefixes_esc() {
        let out = encode(&KeyInput {
            key: Key::Char('b'),
            ctrl: false,
            alt: true,
            shift: false,
        });
        assert_eq!(out, vec![0x1b, 0x62], "Alt+b 应为 ESC 前缀 + 'b'");
    }

    /// Shift+Enter 仅在终端应用启用增强键盘协议后保留 Shift 修饰。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   key = Enter = Unicode code point 13
    ///   Shift modifier value = 1(default) + 1(shift bit) = 2
    ///   enhanced encoding = CSI 13;2u = ESC(1) + "[13;2u"(6) = 7 bytes
    ///   legacy encoding = CR = [0x0d] = 1 byte
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造 `KeyInput { key: Enter, shift: true }`，其余修饰键关闭。
    ///   2. 在 enhanced_keyboard=true 下编码，终端应保留 Shift bit。
    ///   3. 在 enhanced_keyboard=false 下编码，终端应按 legacy 兼容规则回退为 CR。
    ///
    /// 预期结果：
    ///   - 正断言：增强模式精确返回 CSI 13;2u。
    ///   - 正断言：legacy 模式精确返回 CR，避免普通 shell 收到无法识别的 CSI-u 文本。
    ///   - 负断言：增强模式不得退化为裸 CR，否则 TUI 会把 Shift+Enter 当提交键。
    #[test]
    fn test_shift_enter_preserves_modifier_only_in_enhanced_keyboard_mode() {
        let input = KeyInput {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: true,
        };

        let enhanced = encode_for_keyboard_mode(&input, true);
        let legacy = encode_for_keyboard_mode(&input, false);

        assert_eq!(
            enhanced,
            b"\x1b[13;2u".to_vec(),
            "增强键盘模式下 Shift+Enter 应编码为 CSI 13;2u"
        );
        assert_eq!(
            legacy,
            vec![0x0d],
            "legacy 模式应保持兼容并把 Shift+Enter 编码为 CR"
        );
        assert_ne!(
            enhanced,
            vec![0x0d],
            "增强键盘模式不得丢失 Shift 修饰并退化为裸 CR"
        );
    }
}
