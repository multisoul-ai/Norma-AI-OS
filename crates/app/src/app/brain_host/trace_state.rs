//! 大脑活动窗口的状态入口：`brain` 字段对其他模块不可见，界面通过这里读写。

use super::trace::{group_turns, TurnCard};
use crate::app::MultisoulApp;
use brain::acp::trace::AcpTrace;
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

    /// 各项目会话里的记录，分好轮，新的在前。没有项目会话时取自己的。
    pub(crate) fn brain_trace_cards(&self, cx: &gpui::App) -> Vec<TurnCard> {
        let mut entries: Vec<(String, AcpTrace)> = Vec::new();
        for ((_, area_id), session) in &self.desktop_workspace.sessions {
            let app = session.read(cx);
            entries.extend(
                app.brain
                    .trace
                    .items()
                    .map(|t| (area_id.clone(), t.clone())),
            );
        }
        entries.extend(self.brain.trace.items().map(|t| (String::new(), t.clone())));
        group_turns(&entries)
    }

    pub(crate) fn trace_card_expanded(&self, key: &(String, String, u64)) -> bool {
        self.brain.trace_expanded.contains(key)
    }

    pub(crate) fn toggle_trace_card(&mut self, key: (String, String, u64), cx: &mut Context<Self>) {
        if !self.brain.trace_expanded.remove(&key) {
            self.brain.trace_expanded.insert(key);
        }
        cx.notify();
    }
}
