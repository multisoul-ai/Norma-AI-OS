//! Browser card state, commands, and native WebView lifecycle.

use super::MultisoulApp;
use crate::canvas::master_layout::{enter_master_layout_for_area, switch_master_layout_for_area};
use crate::cards::browser::normalize_browser_url;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{Action, AppContext as _, Context, Entity, Subscription, Window};
use std::time::{Duration, Instant};

mod address_input;

mod chrome_auth;
mod chrome_auth_broker;
#[cfg(test)]
#[path = "../../tests/unit/browser_chrome_auth_tests.rs"]
mod chrome_auth_tests;

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
mod cef_backend;
mod engine;
mod load_timeline;
mod location;
pub(crate) mod native;
mod presentation;
mod resource_timing;
#[cfg(target_os = "macos")]
mod scene_clip;
mod store;
mod wry_backend;

pub(crate) use address_input::BrowserAddressInput;
use address_input::BrowserAddressInputEvent;
pub(crate) use chrome_auth::BrowserAuthUiState;
pub(crate) use engine::BrowserEngineKind;
pub(crate) use location::canonical_browser_location;
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
    ImportChromeAuth,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = multisoul, no_json)]
pub(crate) struct BrowserCommand {
    pub(crate) node_id: NodeId,
    pub(crate) command: BrowserCommandKind,
}

pub(crate) struct BrowserAddressEdit {
    pub(crate) node_id: NodeId,
    pub(crate) input: Entity<BrowserAddressInput>,
    _subscription: Subscription,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowserEvent {
    PreviewNavigation {
        node_id: NodeId,
        url: String,
    },
    TitleChanged {
        node_id: NodeId,
        title: String,
    },
    LocationChanged {
        node_id: NodeId,
        url: String,
    },
    PageInteraction {
        node_id: NodeId,
    },
    PageLoad {
        node_id: NodeId,
        phase: load_timeline::BrowserLoadPhase,
        at: Instant,
    },
    NavigateCurrentCard {
        node_id: NodeId,
        url: String,
    },
    ChromeAuthCookies {
        node_id: NodeId,
        url: String,
        batch: chrome_auth::CookieImportBatch,
    },
    ChromeAuthImported {
        node_id: NodeId,
        url: String,
        imported: usize,
        skipped: usize,
    },
    ChromeAuthFailed {
        node_id: NodeId,
        message: String,
    },
}

impl MultisoulApp {
    pub(crate) fn open_terminal_link_in_browser_card(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = url::Url::parse(&url)
            .ok()
            .and_then(|url| url.host_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "网页".to_string());
        if self.route_project_content_to_desktop(
            storage::DesktopContentKind::Browser,
            url.clone(),
            title,
            cx,
        ) {
            return;
        }
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
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("cards.arrange_browser_failed"));
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
                Some(ui::i18n::LocalizedText::new("cards.external_links_macos"));
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

    pub(crate) fn warm_up_browser_engine(&mut self, window: &mut Window) {
        self.browsers.warm_up(window);
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

    pub(crate) fn browser_chrome_auth_state_for(&self, node_id: NodeId) -> BrowserAuthUiState {
        self.browsers.chrome_auth_state(node_id)
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

    pub(crate) fn drain_browser_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for event in self.browsers.drain_events() {
            match event {
                BrowserEvent::TitleChanged { node_id, title } => {
                    if self.set_browser_title(node_id, title) {
                        changed = true;
                    }
                }
                BrowserEvent::PreviewNavigation { node_id, url } => {
                    self.route_workbench_navigation(node_id, url, cx);
                    changed = true;
                }
                BrowserEvent::LocationChanged { node_id, url } => {
                    if self.update_desktop_browser_tab_location(node_id, url.clone(), cx)
                        || self.set_browser_url(node_id, url)
                    {
                        self.mark_canvas_dirty();
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
                BrowserEvent::PageLoad { .. } => {}
                BrowserEvent::NavigateCurrentCard { node_id, url } => {
                    if self.update_desktop_browser_tab_location(node_id, url.clone(), cx)
                        || self.set_browser_url(node_id, url.clone())
                    {
                        self.browsers.load_url(node_id, &url);
                        self.mark_canvas_dirty();
                        changed = true;
                    }
                }
                BrowserEvent::ChromeAuthCookies {
                    node_id,
                    url,
                    batch,
                } => match self.browsers.apply_chrome_auth(node_id, &url, batch) {
                    Ok(Some((imported, skipped))) => {
                        tracing::info!(node_id, imported, skipped, "Chrome Browser auth imported");
                        changed = true;
                    }
                    Ok(None) => changed = true,
                    Err(message) => {
                        self.report_chrome_auth_failure(node_id, &message);
                        changed = true;
                    }
                },
                BrowserEvent::ChromeAuthImported {
                    node_id,
                    url,
                    imported,
                    skipped,
                } => {
                    if self.browsers.complete_chrome_auth(node_id, &url) {
                        tracing::info!(
                            node_id,
                            imported,
                            skipped,
                            "Chrome Browser auth imported asynchronously"
                        );
                        changed = true;
                    }
                }
                BrowserEvent::ChromeAuthFailed { node_id, message } => {
                    self.report_chrome_auth_failure(node_id, &message);
                    changed = true;
                }
            }
        }
        changed
    }

    fn report_chrome_auth_failure(&mut self, node_id: NodeId, message: &str) {
        self.browsers.fail_chrome_auth(node_id, message);
        self.workspace_state.error = Some(
            ui::i18n::LocalizedText::new("cards.chrome_sync_failed").with_arg("message", message),
        );
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
            BrowserCommandKind::ImportChromeAuth => {
                if let Some(node) = self.runtime.document.node(action.node_id) {
                    if let CanvasNodeKind::Browser(spec) = &node.kind {
                        self.browsers.request_chrome_auth(action.node_id, &spec.url);
                    }
                }
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
        if self.update_desktop_browser_tab_location(node_id, url.clone(), cx)
            || self.set_browser_url(node_id, url.clone())
        {
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

#[cfg(test)]
#[path = "../../tests/unit/browser_location_tests.rs"]
mod location_tests;

#[cfg(test)]
#[path = "../../tests/unit/browser_load_timeline_tests.rs"]
mod load_timeline_tests;
