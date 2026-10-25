//! 幕僚活动窗口的开关与焦点：`brain` 字段对其他模块不可见，界面通过这里读写。

use crate::app::MultisoulApp;
use gpui::{Context, FocusHandle, Window};

impl MultisoulApp {
    /// 打开或关闭活动窗口。窗口属于桌面外壳，快捷键落在项目实例上时交给外壳。
    pub(crate) fn toggle_brain_trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |app, cx| app.toggle_brain_trace(window, cx));
            return;
        }
        self.brain.trace_open = !self.brain.trace_open;
        if self.brain.trace_open {
            // 打开即读库，并把动作标为已读：入口上的数字随之清零。
            self.brain.activity.unseen = 0;
            self.refresh_brain_activity(true, cx);
            let handle = self.brain_trace_focus(cx);
            window.focus(&handle);
        } else {
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }

    pub(crate) fn brain_trace_focus(&mut self, cx: &mut Context<Self>) -> FocusHandle {
        self.brain
            .trace_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// 窗口原本是打开的就关掉并返回 `true`。
    pub(crate) fn close_brain_trace(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            return host.update(cx, |app, cx| app.close_brain_trace(cx));
        }
        if !self.brain.trace_open {
            return false;
        }
        self.brain.trace_open = false;
        cx.notify();
        true
    }

    pub(crate) fn brain_trace_open(&self) -> bool {
        self.brain.trace_open
    }
}
