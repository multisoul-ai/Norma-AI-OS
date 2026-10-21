use super::{keycap, label_block, row_shell};
use gpui::{div, prelude::*, px, size, Context, Render, TestAppContext, Window};
use ui::Theme;

struct KeyboardRows;

impl Render for KeyboardRows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        div().w_full().flex().flex_col().children(
            [("short-key", "❖N"), ("long-key", "❖shift-alt-control-A")]
                .map(|(selector, label)| {
                    row_shell(theme)
                        .child(label_block(
                            theme,
                            "Add Project",
                            "Choose a recent project, local folder, or repository for the current workspace",
                        ))
                        .child(
                            keycap(theme, label)
                                .id(selector)
                                .debug_selector(move || selector.into()),
                        )
                }),
        )
    }
}

/// 场景：长修饰键组合不能被说明文字挤到行外，按键列保持右对齐。
/// 数据构造：440px 内容宽度；短标签 ❖N 与长标签 ❖shift-alt-control-A，
/// 长标签应超过旧 88px 最小宽度；每行左右留白 16px。
/// 执行过程：绘制两个生产 row/keycap，读取按键边界并比较宽度与右边缘。
/// 预期结果：长按键自适应增宽；两行右边缘相同且小于 440-16=424，不能溢出。
#[test]
fn settings_keyboard_layout_keycaps_fit_and_align_right() {
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, _| KeyboardRows);
    cx.simulate_resize(size(px(440.0), px(300.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let short = cx.debug_bounds("short-key").expect("应渲染短按键");
    let long = cx.debug_bounds("long-key").expect("应渲染长按键");

    assert!(
        long.size.width > px(88.0),
        "长组合应按文本增宽，不能固定 88px"
    );
    assert_eq!(
        short.right(),
        long.right(),
        "按键列应右对齐，与标签长度无关"
    );
    assert!(
        long.right() <= px(424.0),
        "说明文字必须收缩换行，不能把按键挤出行外：{long:?}"
    );
    assert!(short.left() > px(16.0), "按键不能覆盖行首标题");
}

/// 场景：录制冲突快捷键时，错误必须出现在正在录制的那一行，而不是只出现在可能已滚出视野的页顶。
/// 数据构造：默认状态；对 NewTerminal 开始录制，再录入已被 FocusCommandInput 占用的 cmd-k。
/// 执行过程：读取 NewTerminal 与另一行 NextWorkspace 的行内提示。
/// 预期结果：被录制行的提示是错误且不再是静态说明；其他行保持静态说明且非错误。
#[test]
fn recording_row_shows_validation_error_inline() {
    use crate::shortcuts::{ShortcutCommandId, ShortcutSettingsState};
    let mut state = ShortcutSettingsState::default();
    state.begin_recording(ShortcutCommandId::NewTerminal);
    let consumed = state.record_keystroke(&gpui::Keystroke::parse("cmd-k").unwrap());
    assert!(consumed, "录制态下冲突按键仍应被消费，不能漏给其他处理器");
    assert_eq!(
        state.recording,
        Some(ShortcutCommandId::NewTerminal),
        "冲突后应继续录制，让用户直接换一个组合"
    );

    let entry = |command| {
        state
            .effective
            .iter()
            .find(|entry| entry.command == command)
            .expect("默认注册表应含该命令")
    };
    let recording = super::row_hint(entry(ShortcutCommandId::NewTerminal), &state);
    let other = super::row_hint(entry(ShortcutCommandId::NextWorkspace), &state);

    assert!(recording.is_error, "被录制行应显示错误态提示");
    assert!(
        recording.text != ui::i18n::text(entry(ShortcutCommandId::NewTerminal).hint),
        "被录制行的提示应替换为校验错误：{}",
        recording.text
    );
    assert!(
        !recording.text.contains("shortcuts."),
        "校验错误不得泄漏本地化 key：{}",
        recording.text
    );
    assert!(
        recording
            .text
            .contains(ui::i18n::text("shortcuts.focus_command_input")),
        "冲突提示应点名占用该组合的命令：{}",
        recording.text
    );
    assert!(!other.is_error, "其他行不应显示错误");
    assert_eq!(
        other.text,
        ui::i18n::text(entry(ShortcutCommandId::NextWorkspace).hint),
        "其他行保持静态说明"
    );
}
