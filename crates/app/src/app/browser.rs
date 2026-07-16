//! Browser card state, commands, and native WebView lifecycle.

use super::MultisoulApp;
use crate::canvas::master_layout::{enter_master_layout_for_area, switch_master_layout_for_area};
use crate::cards::browser::normalize_browser_url;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{Action, AppContext as _, Context, Entity, Subscription, Window};
use std::collections::HashSet;
use std::time::Duration;

mod address_input;

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
mod cef_backend;
mod engine;
mod native;
mod store;
mod wry_backend;

pub(crate) use address_input::BrowserAddressInput;
use address_input::BrowserAddressInputEvent;
pub(crate) use engine::BrowserEngineKind;
pub(crate) use native::{browser_native_view_bounds, BrowserViewBounds, NativeBrowserView};
pub(crate) use store::BrowserStore;
pub(crate) use wry_backend::initialize_safari_browser_identity;

const CEF_MESSAGE_PUMP_INTERVAL: Duration = Duration::from_millis(16);

pub(crate) fn native_browser_views_visible(settings_panel_open: bool) -> bool {
    !settings_panel_open
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserCommandKind {
    Back,
    Forward,
    Reload,
    EditAddress,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = multisoul, no_json)]
pub(crate) struct BrowserCommand {
    pub(crate) node_id: NodeId,
    pub(crate) command: BrowserCommandKind,
}

pub(crate) struct BrowserAddressEdit {
    pub(crate) node_id: NodeId,
    input: Entity<BrowserAddressInput>,
    _subscription: Subscription,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowserEvent {
    TitleChanged { node_id: NodeId, title: String },
    PageInteraction { node_id: NodeId },
    NavigateCurrentCard { node_id: NodeId, url: String },
}

impl MultisoulApp {
    pub(crate) fn open_terminal_link_in_browser_card(&mut self, url: String, window: &mut Window) {
        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let node_id = self.runtime.spawn_browser(url, None, bounds);
        self.motion.spawn_node(node_id, bounds);
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let arranged = if self.master_layout.is_active() {
            switch_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        } else {
            enter_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        };
        if arranged {
            self.mark_canvas_dirty();
        } else {
            self.workspace_state.error = Some("Could not arrange browser link card.".to_string());
        }
    }

    pub(crate) fn open_url_in_default_browser(&mut self, url: String) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::thread::Builder::new()
                .name("open_terminal_link_in_default_browser".to_string())
                .spawn(move || {
                    if let Err(error) = std::process::Command::new("open").arg(url).spawn() {
                        tracing::warn!(%error, "failed to open terminal link in default browser");
                    }
                });
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.workspace_state.error =
                Some("Opening terminal links externally is only available on macOS.".to_string());
        }
    }

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

    pub(crate) fn browser_address_input_for(
        &self,
        node_id: NodeId,
    ) -> Option<Entity<BrowserAddressInput>> {
        self.browser_address_edit
            .as_ref()
            .filter(|edit| edit.node_id == node_id)
            .map(|edit| edit.input.clone())
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

    pub(crate) fn release_browser_input(&mut self, node_id: NodeId) {
        self.browsers.release_input(node_id);
    }

    pub(crate) fn release_all_browser_input(&mut self) {
        self.browsers.release_all_input();
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
                BrowserEvent::PageInteraction { node_id } => {
                    if self.runtime.document.node(node_id).is_some() {
                        self.runtime.selection.select_only(node_id);
                        self.runtime.focus.exit_to_canvas();
                        self.interaction.keyboard_selected_node = None;
                        self.mark_node_viewed_if_needed(node_id);
                        changed = true;
                    }
                }
                BrowserEvent::NavigateCurrentCard { node_id, url } => {
                    if self.set_browser_url(node_id, url.clone()) {
                        self.browsers.load_url(node_id, &url);
                        self.mark_canvas_dirty();
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_all_browser_input();
        match action.command {
            BrowserCommandKind::Back => self.browsers.go_back(action.node_id),
            BrowserCommandKind::Forward => self.browsers.go_forward(action.node_id),
            BrowserCommandKind::Reload => self.browsers.reload(action.node_id),
            BrowserCommandKind::EditAddress => {
                self.begin_browser_address_edit(action.node_id, window, cx)
            }
        }
        cx.notify();
    }

    fn begin_browser_address_edit(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = self
            .browser_address_edit
            .as_ref()
            .filter(|edit| edit.node_id == node_id)
        {
            edit.input.update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        let Some(node) = self.runtime.document.node(node_id) else {
            return;
        };
        let CanvasNodeKind::Browser(spec) = &node.kind else {
            return;
        };
        let input = cx.new(|cx| BrowserAddressInput::new(spec.url.clone(), self.theme, cx));
        let subscription = cx.subscribe(&input, move |app, input, event, cx| match event {
            BrowserAddressInputEvent::Submit => {
                let draft = input.read(cx).value();
                app.submit_browser_address_edit(node_id, &draft, cx);
            }
            BrowserAddressInputEvent::Cancel => {
                app.cancel_browser_address_edit(node_id, cx);
            }
        });
        self.browsers.release_input(node_id);
        input.update(cx, |input, cx| input.focus(window, cx));
        self.browser_address_edit = Some(BrowserAddressEdit {
            node_id,
            input,
            _subscription: subscription,
        });
    }

    fn submit_browser_address_edit(
        &mut self,
        node_id: NodeId,
        draft: &str,
        cx: &mut Context<Self>,
    ) {
        if self
            .browser_address_edit
            .as_ref()
            .map(|edit| edit.node_id != node_id)
            .unwrap_or(true)
        {
            return;
        }
        let url = normalize_browser_url(draft);
        self.browser_address_edit = None;
        if self.set_browser_url(node_id, url.clone()) {
            self.browsers.load_url(node_id, &url);
            self.mark_canvas_dirty();
        }
        self.browsers.release_input(node_id);
        self.focus_app_root(cx);
        cx.notify();
    }

    fn cancel_browser_address_edit(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        if self
            .browser_address_edit
            .as_ref()
            .map(|edit| edit.node_id == node_id)
            .unwrap_or(false)
        {
            self.browser_address_edit = None;
            self.browsers.release_input(node_id);
            self.focus_app_root(cx);
            cx.notify();
        }
    }

    fn focus_app_root(&self, cx: &mut Context<Self>) {
        let Some(window_handle) = cx.active_window() else {
            return;
        };
        let focus_handle = self.focus_handle.clone();
        let _ = cx.update_window(window_handle, |_, window, _| focus_handle.focus(window));
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

#[cfg(test)]
#[path = "../../tests/unit/browser_cef_tests.rs"]
mod cef_tests;

#[cfg(test)]
#[path = "../../tests/unit/browser_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/browser_overlay_visibility_tests.rs"]
mod overlay_visibility_tests;

#[cfg(test)]
#[path = "../../tests/unit/browser_interaction_visual_tests.rs"]
mod interaction_visual_tests;
