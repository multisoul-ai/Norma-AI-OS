//! 岛面板上的输入框只要自己成为 key window。
//! 不要调用 `cx.activate(true)`：那是异步的 `activateIgnoringOtherApps`，
//! 结束后系统会把 key 还给工作空间主窗口，按键就进不了这块 NonactivatingPanel。

use gpui::Window;

pub(super) fn focus_panel_input(window: &mut Window) {
    window.activate_window();
}
