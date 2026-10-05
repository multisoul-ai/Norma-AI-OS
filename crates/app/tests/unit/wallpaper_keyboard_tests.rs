use super::action::WallpaperAction;
use gpui::{div, prelude::*, Context, Modifiers, Render, TestAppContext, Window};
use std::{cell::Cell, rc::Rc};
use ui::component::{button::Button, Disableable};

struct ActionFixture {
    activations: Rc<Cell<usize>>,
    enabled: Rc<Cell<bool>>,
}

impl Render for ActionFixture {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let activations = self.activations.clone();
        div().child(
            Button::new("wallpaper-keyboard-action")
                .debug_selector(|| "wallpaper-keyboard-action".to_owned())
                .label("Choose wallpaper")
                .disabled(!self.enabled.get())
                .on_wallpaper_activate(self.enabled.get(), move |_, _, _| {
                    activations.set(activations.get() + 1);
                }),
        )
    }
}

/// WP-09：壁纸操作通过 Enter、空格和鼠标各执行一次，禁用后不再执行。
///
/// 数据构造（含关键数值的推导过程）：
///   单个按钮与计数器；Enter(1) + 空格(1) + 点击(1) = 3 次有效激活。
///   禁用后的 Enter、空格与点击均为 0 次，最终仍为 3。
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立窗口焦点，Tab 到按钮；Escape 不激活。
///   2. 用 Enter、空格、点击依次执行同一个回调。
///   3. 保持原焦点但禁用按钮并重绘，再发送三种输入。
/// 预期结果：
///   - 正断言：启用时每种输入恰好增加一次计数。
///   - 负断言：Escape 和禁用后的输入不得改变计数。
#[test]
fn wallpaper_actions_activate_once_and_respect_disabled_state() {
    let activations = Rc::new(Cell::new(0));
    let enabled = Rc::new(Cell::new(true));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|_| ActionFixture {
            activations: activations.clone(),
            enabled: enabled.clone(),
        });
        ui::component::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    assert_eq!(activations.get(), 0, "Escape 不得选择壁纸");
    cx.simulate_keystrokes("enter");
    assert_eq!(activations.get(), 1, "Enter 必须且只能执行一次回调");
    cx.simulate_keystrokes("space");
    assert_eq!(activations.get(), 2, "空格必须且只能执行一次回调");
    let bounds = cx
        .debug_bounds("wallpaper-keyboard-action")
        .expect("按钮必须可见");
    cx.simulate_click(bounds.center(), Modifiers::none());
    assert_eq!(activations.get(), 3, "补齐键盘行为后鼠标应继续工作");
    enabled.set(false);
    cx.update(|window, cx| {
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    cx.simulate_keystrokes("enter space");
    assert_eq!(activations.get(), 3, "禁用按钮不能通过遗留焦点触发键盘操作");
    cx.simulate_click(bounds.center(), Modifiers::none());
    assert_eq!(activations.get(), 3, "禁用按钮不能通过鼠标执行操作");
}
