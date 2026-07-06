//! Browser card state, commands, and native WebView lifecycle.

use super::{BrowserCommand, BrowserCommandKind, MultisoulApp};
use crate::cards::browser::normalize_browser_url;
use canvas::{CanvasNodeKind, NodeId};
use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::{Context, KeyDownEvent, Window};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
mod cef_backend;
mod engine;
mod native;
mod wry_backend;

pub(crate) use engine::BrowserEngineKind;
pub(crate) use native::{browser_native_view_bounds, BrowserViewBounds, NativeBrowserView};

const CEF_MESSAGE_PUMP_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrowserAddressEdit {
    pub(crate) node_id: NodeId,
    pub(crate) draft: String,
    replace_on_input: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowserEvent {
    TitleChanged { node_id: NodeId, title: String },
}

pub(crate) struct BrowserStore {
    views: HashMap<NodeId, BrowserView>,
    sender: Sender<BrowserEvent>,
    receiver: Receiver<BrowserEvent>,
}

struct BrowserView {
    url: String,
    bounds: BrowserViewBounds,
    visible: bool,
    native_view: Option<NativeBrowserView>,
}

impl Default for BrowserStore {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            views: HashMap::new(),
            sender,
            receiver,
        }
    }
}

impl BrowserStore {
    pub(crate) fn drain_events(&self) -> Vec<BrowserEvent> {
        self.receiver.try_iter().collect()
    }

    pub(crate) fn sync_view(
        &mut self,
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
    ) {
        if bounds.width <= 1.0 || bounds.height <= 1.0 {
            self.hide(node_id);
            return;
        }

        let event_sender = self.sender.clone();
        let view = self.views.entry(node_id).or_insert_with(|| BrowserView {
            url: url.to_string(),
            bounds,
            visible: false,
            native_view: None,
        });
        view.sync(node_id, url, bounds, window, event_sender);
    }

    pub(crate) fn load_url(&mut self, node_id: NodeId, url: &str) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.url = url.to_string();
            view.load_url(url);
        }
    }

    pub(crate) fn go_back(&mut self, node_id: NodeId) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.go_back();
        }
    }

    pub(crate) fn go_forward(&mut self, node_id: NodeId) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.go_forward();
        }
    }

    pub(crate) fn reload(&mut self, node_id: NodeId) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.reload();
        }
    }

    pub(crate) fn hide(&mut self, node_id: NodeId) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.set_visible(false);
        }
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
        if let Some(mut view) = self.views.remove(&node_id) {
            view.set_visible(false);
        }
    }

    pub(crate) fn retain_visible(&mut self, visible: &HashSet<NodeId>) {
        for (node_id, view) in self.views.iter_mut() {
            if !visible.contains(node_id) {
                view.set_visible(false);
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        for view in self.views.values_mut() {
            view.set_visible(false);
        }
        self.views.clear();
    }
}

impl BrowserView {
    fn sync(
        &mut self,
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) {
        if self.native_view.is_none() {
            let configured = std::env::var("MULTISOUL_BROWSER_ENGINE").ok();
            let engine = BrowserEngineKind::from_env_value(configured.as_deref());
            match NativeBrowserView::create(engine, node_id, url, bounds, window, event_sender) {
                Ok(native_view) => self.native_view = Some(native_view),
                Err(error) => tracing::warn!(
                    node_id,
                    engine = ?engine,
                    error = %error,
                    "failed to create native Browser view"
                ),
            }
        }

        if self.bounds != bounds {
            self.bounds = bounds;
            self.set_bounds(bounds);
        }
        if self.url != url {
            self.url = url.to_string();
            self.load_url(url);
        }
        self.set_visible(true);
    }

    fn load_url(&mut self, url: &str) {
        if let Some(native_view) = &mut self.native_view {
            native_view.load_url(url);
        }
    }

    fn go_back(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.go_back();
        }
    }

    fn go_forward(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.go_forward();
        }
    }

    fn reload(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.reload();
        }
    }

    fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        if let Some(native_view) = &mut self.native_view {
            native_view.set_bounds(bounds);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if let Some(native_view) = &mut self.native_view {
            native_view.set_visible(visible);
        }
    }
}

impl MultisoulApp {
    pub(crate) fn start_cef_message_pump_loop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(CEF_MESSAGE_PUMP_INTERVAL)
                .await;
            let go = this.update_in(cx, |_app, _window, _cx| {
                pump_cef_message_loop_if_initialized();
            });
            if go.is_err() {
                break;
            }
        })
        .detach();
    }

    pub(crate) fn browser_address_draft_for(&self, node_id: NodeId) -> Option<&str> {
        self.browser_address_edit
            .as_ref()
            .filter(|edit| edit.node_id == node_id)
            .map(|edit| edit.draft.as_str())
    }

    pub(crate) fn sync_browser_view_for_node(
        &mut self,
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
    ) {
        self.browsers.sync_view(node_id, url, bounds, window);
    }

    pub(crate) fn finish_browser_view_sync(&mut self, visible: &HashSet<NodeId>) {
        self.browsers.retain_visible(visible);
    }

    pub(crate) fn remove_browser_view(&mut self, node_id: NodeId) {
        self.browsers.remove(node_id);
        if self
            .browser_address_edit
            .as_ref()
            .map(|edit| edit.node_id == node_id)
            .unwrap_or(false)
        {
            self.browser_address_edit = None;
        }
    }

    pub(crate) fn clear_browser_views(&mut self) {
        self.browsers.clear();
        self.browser_address_edit = None;
    }

    pub(crate) fn drain_browser_events(&mut self) -> bool {
        let mut changed = false;
        for event in self.browsers.drain_events() {
            match event {
                BrowserEvent::TitleChanged { node_id, title } => {
                    if self.set_browser_title(node_id, title) {
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub(crate) fn on_browser_command(
        &mut self,
        action: &BrowserCommand,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            BrowserCommandKind::Back => self.browsers.go_back(action.node_id),
            BrowserCommandKind::Forward => self.browsers.go_forward(action.node_id),
            BrowserCommandKind::Reload => self.browsers.reload(action.node_id),
            BrowserCommandKind::EditAddress => self.begin_browser_address_edit(action.node_id),
        }
        cx.notify();
    }

    pub(crate) fn handle_browser_address_key_down(
        &mut self,
        ev: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.browser_address_edit.is_none() {
            return false;
        }
        if browser_address_paste_requested(ev) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.replace_browser_address_draft(&text);
            }
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        if ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control {
            return false;
        }

        match ev.keystroke.key.as_str() {
            "enter" => {
                self.submit_browser_address_edit();
                cx.stop_propagation();
                cx.notify();
                true
            }
            "escape" => {
                self.browser_address_edit = None;
                cx.stop_propagation();
                cx.notify();
                true
            }
            "backspace" => {
                if let Some(edit) = &mut self.browser_address_edit {
                    if edit.replace_on_input {
                        edit.draft.clear();
                        edit.replace_on_input = false;
                    } else {
                        edit.draft.pop();
                    }
                }
                cx.stop_propagation();
                cx.notify();
                true
            }
            _ => {
                if let Some(ch) = browser_input_char(ev) {
                    self.push_browser_address_char(ch);
                    cx.stop_propagation();
                    cx.notify();
                    return true;
                }
                false
            }
        }
    }

    fn begin_browser_address_edit(&mut self, node_id: NodeId) {
        let Some(node) = self.runtime.document.node(node_id) else {
            return;
        };
        let CanvasNodeKind::Browser(spec) = &node.kind else {
            return;
        };
        self.browser_address_edit = Some(BrowserAddressEdit {
            node_id,
            draft: spec.url.clone(),
            replace_on_input: true,
        });
    }

    fn push_browser_address_char(&mut self, ch: char) {
        if let Some(edit) = &mut self.browser_address_edit {
            if edit.replace_on_input {
                edit.draft.clear();
                edit.replace_on_input = false;
            }
            edit.draft.push(ch);
        }
    }

    fn replace_browser_address_draft(&mut self, text: &str) {
        let Some(edit) = &mut self.browser_address_edit else {
            return;
        };
        let pasted = text.lines().next().unwrap_or("").trim();
        if pasted.is_empty() {
            return;
        }
        edit.draft.clear();
        edit.draft.push_str(pasted);
        edit.replace_on_input = false;
    }

    fn submit_browser_address_edit(&mut self) {
        let Some(edit) = self.browser_address_edit.take() else {
            return;
        };
        let url = normalize_browser_url(&edit.draft);
        if self.set_browser_url(edit.node_id, url.clone()) {
            self.browsers.load_url(edit.node_id, &url);
            self.mark_canvas_dirty();
        }
    }

    fn set_browser_url(&mut self, node_id: NodeId, url: String) -> bool {
        let Some(node) = self.runtime.document.node_mut(node_id) else {
            return false;
        };
        let CanvasNodeKind::Browser(spec) = &mut node.kind else {
            return false;
        };
        if spec.url == url && spec.title.is_none() {
            return false;
        }
        spec.url = url;
        spec.title = None;
        true
    }

    fn set_browser_title(&mut self, node_id: NodeId, title: String) -> bool {
        let Some(node) = self.runtime.document.node_mut(node_id) else {
            return false;
        };
        let CanvasNodeKind::Browser(spec) = &mut node.kind else {
            return false;
        };
        let clean = title.trim().to_string();
        if clean.is_empty() || spec.title.as_deref() == Some(clean.as_str()) {
            return false;
        }
        spec.title = Some(clean);
        self.mark_canvas_dirty();
        true
    }
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn pump_cef_message_loop_if_initialized() -> bool {
    cef_backend::pump_cef_message_loop_if_initialized()
}

#[cfg(not(all(target_os = "macos", feature = "chromium-browser")))]
fn pump_cef_message_loop_if_initialized() -> bool {
    false
}

fn browser_input_char(ev: &KeyDownEvent) -> Option<char> {
    if ev.keystroke.modifiers.alt || ev.keystroke.modifiers.function {
        return None;
    }
    if let Some(key_char) = &ev.keystroke.key_char {
        let mut chars = key_char.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if is_browser_address_char(ch) {
                return Some(ch);
            }
        }
    }
    let mut chars = ev.keystroke.key.chars();
    let ch = chars.next()?;
    if chars.next().is_none() && is_browser_address_char(ch) {
        return Some(ch);
    }
    None
}

fn browser_address_paste_requested(ev: &KeyDownEvent) -> bool {
    ev.keystroke.key == "v"
        && ev.keystroke.modifiers.platform
        && !ev.keystroke.modifiers.control
        && !ev.keystroke.modifiers.alt
        && !ev.keystroke.modifiers.shift
        && !ev.keystroke.modifiers.function
}

fn is_browser_address_char(ch: char) -> bool {
    ch.is_ascii_graphic()
}

#[cfg(test)]
#[path = "../../tests/unit/browser_cef_tests.rs"]
mod cef_tests;

#[cfg(test)]
#[path = "../../tests/unit/browser_view_tests.rs"]
mod tests;
