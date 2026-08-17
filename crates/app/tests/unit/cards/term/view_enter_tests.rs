use super::*;
use terminal::encode;

/// Enter 即使携带 key_char，也必须按终端控制键处理而不是文本换行。
///
/// 数据构造（含关键数值的推导过程）：
///   enter_lf.key = "enter"，key_char = Some("\n")，GPUI/macOS 可能把 Enter 字符化为 LF(0x0a)
///   enter_cr.key = "enter"，key_char = Some("\r")，另一类系统事件可能给 CR(0x0d)
///   终端提交应使用 Key::Enter → encode = [0x0d]，而不是 Key::Char('\n') → encode = [0x0a]
///
/// 执行过程（逐步说明系统如何处理）：
///   1. keystroke_to_input(enter_lf) → 应优先匹配 ks.key = "enter"
///   2. keystroke_to_input(enter_cr) → 应同样返回 Key::Enter
///   3. encode(result) → 两者都应写入 CR(0x0d)
///
/// 预期结果：
///   - 正断言：LF/CR 两种 key_char 都返回 Key::Enter
///   - 正断言：两者编码均为 [0x0d]，Codex/Claude Code 可据此提交输入
///   - 负断言：LF 场景不应返回 Key::Char('\n') 或编码为 [0x0a]，避免 TUI 文本区换行
///   - 负断言：CR 场景不应返回 Key::Char('\r')，控制键语义不能被 key_char 抢走
#[test]
fn test_enter_with_key_char_maps_to_enter_not_text_newline() {
    use gpui::{Keystroke, Modifiers};

    let enter_lf = Keystroke {
        key: "enter".into(),
        key_char: Some("\n".into()),
        modifiers: Modifiers::default(),
    };
    let enter_cr = Keystroke {
        key: "enter".into(),
        key_char: Some("\r".into()),
        modifiers: Modifiers::default(),
    };

    let lf_result = keystroke_to_input(&enter_lf);
    let cr_result = keystroke_to_input(&enter_cr);

    assert_eq!(
        lf_result,
        Some(KeyInput {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        }),
        "Enter + key_char LF 应保持 Key::Enter，而不是文本换行"
    );
    assert_eq!(
        cr_result,
        Some(KeyInput {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        }),
        "Enter + key_char CR 应保持 Key::Enter，控制键语义优先"
    );
    assert_eq!(
        encode(&lf_result.expect("LF enter should produce KeyInput")),
        vec![0x0d],
        "Enter + LF key_char 最终应编码为 CR(0x0d) 以提交 TUI 输入"
    );
    assert_eq!(
        encode(&cr_result.expect("CR enter should produce KeyInput")),
        vec![0x0d],
        "Enter + CR key_char 最终应编码为 CR(0x0d)"
    );
    assert_ne!(
        lf_result,
        Some(KeyInput {
            key: Key::Char('\n'),
            ctrl: false,
            alt: false,
            shift: false,
        }),
        "Enter 不应退化为 Key::Char('\\n')，否则 Codex/Claude Code 会把它当文本换行"
    );
    assert_ne!(
        cr_result,
        Some(KeyInput {
            key: Key::Char('\r'),
            ctrl: false,
            alt: false,
            shift: false,
        }),
        "Enter 不应退化为 Key::Char('\\r')，控制键和文本字符需保持区分"
    );
}
