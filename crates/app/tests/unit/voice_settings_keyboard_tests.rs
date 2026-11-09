use gpui::{Modifiers, TestAppContext};

/// 设置操作通过 Enter、空格和鼠标触发同一回调，其他按键不触发操作。
///
/// 数据构造（含关键数值的推导过程）：
///   单按钮窗口，操作次数初始为 0；Enter(1)+空格(1)+鼠标(1)=3 次操作。
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立窗口初始焦点后用 Tab 遍历按钮；Escape 不触发操作。
///   2. Enter 与空格分别执行一次回调；鼠标点击仍执行同一回调。
///
/// 预期结果：
///   - 正断言：三种激活方式各增加一次计数。
///   - 负断言：Escape 不激活按钮，键盘事件不会重复执行回调。
#[test]
fn voice_redesign_actions_support_keyboard_and_mouse() {
    use crate::settings::voice::view::components::VoiceAction;
    use gpui::{div, prelude::*, Context, Render, Window};
    use std::{cell::Cell, rc::Rc};
    use ui::component::button::Button;

    struct ActionFixture(Rc<Cell<usize>>);
    impl Render for ActionFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let activations = self.0.clone();
            div().child(
                Button::new("voice-keyboard-action")
                    .debug_selector(|| "voice-keyboard-action".to_string())
                    .label("Activate")
                    .on_activate(move |_, _, _| activations.set(activations.get() + 1)),
            )
        }
    }
    let activations = Rc::new(Cell::new(0));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|_| ActionFixture(activations.clone()));
        ui::component::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    assert_eq!(activations.get(), 0, "Escape 不能执行设置操作");
    cx.simulate_keystrokes("enter");
    assert_eq!(activations.get(), 1, "Enter 应恰好激活一次设置操作");
    cx.simulate_keystrokes("space");
    assert_eq!(activations.get(), 2, "空格应激活一次，不得重复处理键盘事件");
    let action = cx
        .debug_bounds("voice-keyboard-action")
        .expect("应有设置按钮");
    cx.simulate_click(action.center(), Modifiers::none());
    assert_eq!(activations.get(), 3, "补充键盘行为后鼠标点击仍应正常");
}

/// 播报开关可通过键盘切换，状态和鼠标操作共用同一回调。
///
/// 数据构造（含关键数值的推导过程）：
///   初始关闭；空格(1)+Enter(1)+鼠标(1)=3 次切换，最终应开启。
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立窗口初始焦点后用 Tab 遍历开关；Escape 不执行回调。
///   2. 空格开启，重绘后 Enter 关闭，再重绘后鼠标开启。
///
/// 预期结果：
///   - 正断言：键盘与鼠标分别切换实际受控值，回调累计三次。
///   - 负断言：Escape 不切换，Enter 不重复执行且不保留过期值。
#[test]
fn voice_redesign_switch_supports_keyboard_and_mouse() {
    use crate::settings::voice::view::spoken_reply::toggle_row;
    use gpui::{prelude::*, Context, Render, Window};
    use std::{cell::Cell, rc::Rc};

    struct SwitchFixture(Rc<Cell<bool>>, Rc<Cell<usize>>);
    impl Render for SwitchFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let value = self.0.clone();
            let calls = self.1.clone();
            toggle_row(
                ui::Theme::dark(),
                "Announcement",
                "Toggle spoken replies",
                value.get(),
                "voice-keyboard-switch",
                move |next, _, cx| {
                    value.set(*next);
                    calls.set(calls.get() + 1);
                    cx.refresh_windows();
                },
            )
        }
    }
    let value = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|_| SwitchFixture(value.clone(), calls.clone()));
        ui::component::Root::new(view, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    assert_eq!(calls.get(), 0, "Escape 不能切换播报开关");
    cx.simulate_keystrokes("space");
    assert!(value.get(), "空格应开启实际受控值");
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("enter");
    assert!(!value.get(), "重绘后 Enter 应关闭，不得使用上次的过期值");
    assert_eq!(calls.get(), 2, "两种键盘激活必须各执行一次");
    cx.update(|window, cx| window.draw(cx).clear());
    let toggle = cx
        .debug_bounds("voice-keyboard-switch")
        .expect("应保留开关");
    cx.simulate_click(toggle.center(), Modifiers::none());
    assert!(value.get(), "键盘操作后鼠标仍应开启开关");
    assert_eq!(calls.get(), 3, "鼠标操作不得重复执行开关回调");
}
