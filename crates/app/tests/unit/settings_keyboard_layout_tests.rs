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
