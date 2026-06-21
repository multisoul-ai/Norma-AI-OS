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
        Key::Up => vec![0x1b, b'[', b'A'],
        Key::Down => vec![0x1b, b'[', b'B'],
        Key::Right => vec![0x1b, b'[', b'C'],
        Key::Left => vec![0x1b, b'[', b'D'],
    }
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
                alt: false
            }),
            vec![0x0d],
            "Enter → CR"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Tab,
                ctrl: false,
                alt: false
            }),
            vec![0x09],
            "Tab → HT"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Backspace,
                ctrl: false,
                alt: false
            }),
            vec![0x7f],
            "Backspace → DEL"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Escape,
                ctrl: false,
                alt: false
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
        });
        assert_eq!(up, vec![0x1b, b'[', b'A'], "Up 应为 CSI A 序列");
        assert_eq!(up.len(), 3, "方向键应是 3 字节 CSI 序列");
        assert_eq!(
            encode(&KeyInput {
                key: Key::Down,
                ctrl: false,
                alt: false
            }),
            vec![0x1b, b'[', b'B'],
            "Down → CSI B"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Right,
                ctrl: false,
                alt: false
            }),
            vec![0x1b, b'[', b'C'],
            "Right → CSI C"
        );
        assert_eq!(
            encode(&KeyInput {
                key: Key::Left,
                ctrl: false,
                alt: false
            }),
            vec![0x1b, b'[', b'D'],
            "Left → CSI D"
        );
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
        });
        assert_eq!(out, vec![0x1b, 0x62], "Alt+b 应为 ESC 前缀 + 'b'");
    }
}
