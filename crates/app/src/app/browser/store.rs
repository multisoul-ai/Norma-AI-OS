//! Browser 原生视图的同步、可见性与输入焦点生命周期。

use super::chrome_auth::{BrowserAuthUiState, CookieImportBatch};
#[cfg(feature = "e2e-test-support")]
use super::chrome_auth::{BrowserCookie, SameSitePolicy};
use super::chrome_auth_broker::request_chrome_cookies;
use super::native::CookieImportStatus;
use super::{
    canonical_browser_location, BrowserEngineKind, BrowserEvent, BrowserViewBounds,
    NativeBrowserView,
};
use canvas::NodeId;
use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::Window;
use std::collections::{HashMap, HashSet};

pub(crate) struct BrowserStore {
    views: HashMap<NodeId, BrowserView>,
    sender: Sender<BrowserEvent>,
    receiver: Receiver<BrowserEvent>,
    native_views_enabled: bool,
    auth_states: HashMap<NodeId, BrowserAuthUiState>,
    #[cfg(feature = "e2e-test-support")]
    auth_fake_result: Option<Result<(usize, usize), String>>,
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
            native_views_enabled: true,
            auth_states: HashMap::new(),
            #[cfg(feature = "e2e-test-support")]
            auth_fake_result: None,
        }
    }
}

impl BrowserStore {
    pub(crate) fn drain_events(&mut self) -> Vec<BrowserEvent> {
        let mut events = self.receiver.try_iter().collect::<Vec<_>>();
        events.extend(self.views.iter().filter_map(|(node_id, view)| {
            view.current_url_for_polling()
                .map(|url| BrowserEvent::LocationChanged {
                    node_id: *node_id,
                    url,
                })
        }));
        events
            .into_iter()
            .filter_map(|event| match event {
                BrowserEvent::LocationChanged { node_id, url } => {
                    self.accept_location_change(node_id, url)
                }
                event => Some(event),
            })
            .collect()
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
        if self.native_views_enabled {
            view.sync(node_id, url, bounds, window, event_sender);
        }
    }

    pub(crate) fn disable_native_views(&mut self) {
        self.native_views_enabled = false;
    }

    fn accept_location_change(&mut self, node_id: NodeId, url: String) -> Option<BrowserEvent> {
        let normalized_url = canonical_browser_location(&url);
        let view = self.views.get_mut(&node_id)?;
        if view.url == normalized_url {
            return None;
        }
        view.url = normalized_url.clone();
        self.auth_states.remove(&node_id);
        Some(BrowserEvent::LocationChanged {
            node_id,
            url: normalized_url,
        })
    }

    pub(crate) fn load_url(&mut self, node_id: NodeId, url: &str) {
        self.auth_states.remove(&node_id);
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
        self.release_input(node_id);
        if let Some(view) = self.views.get_mut(&node_id) {
            view.set_visible(false);
        }
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
        self.auth_states.remove(&node_id);
        if let Some(mut view) = self.views.remove(&node_id) {
            view.release_input();
            view.set_visible(false);
        }
    }

    pub(crate) fn release_input(&mut self, node_id: NodeId) {
        if let Some(view) = self.views.get_mut(&node_id) {
            view.release_input();
        }
    }

    pub(crate) fn release_all_input(&mut self) {
        for view in self.views.values_mut() {
            view.release_input();
        }
    }

    pub(crate) fn retain_visible(&mut self, visible: &HashSet<NodeId>) {
        let hidden_nodes = self
            .views
            .keys()
            .filter(|node_id| !visible.contains(node_id))
            .copied()
            .collect::<Vec<_>>();
        for node_id in hidden_nodes {
            self.hide(node_id);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.release_all_input();
        for view in self.views.values_mut() {
            view.set_visible(false);
        }
        self.views.clear();
        self.auth_states.clear();
    }

    pub(crate) fn chrome_auth_state(&self, node_id: NodeId) -> BrowserAuthUiState {
        self.auth_states.get(&node_id).copied().unwrap_or_default()
    }

    pub(crate) fn request_chrome_auth(&mut self, node_id: NodeId, url: &str) {
        if self.chrome_auth_state(node_id) == BrowserAuthUiState::Waiting {
            return;
        }
        self.auth_states
            .insert(node_id, BrowserAuthUiState::Waiting);
        #[cfg(feature = "e2e-test-support")]
        if let Some(result) = self.auth_fake_result.clone() {
            match result {
                Ok((imported, skipped)) => {
                    let cookies = (0..imported)
                        .map(|index| BrowserCookie {
                            name: format!("e2e_session_{index}"),
                            value: "[e2e-redacted]".to_string(),
                            domain: url::Url::parse(url)
                                .ok()
                                .and_then(|url| url.host_str().map(str::to_string))
                                .unwrap_or_else(|| "invalid.test".to_string()),
                            host_only: true,
                            path: "/".to_string(),
                            secure: url.starts_with("https://"),
                            http_only: true,
                            same_site: SameSitePolicy::Lax,
                            expires_unix_seconds: None,
                        })
                        .collect();
                    let _ = self.sender.send(BrowserEvent::ChromeAuthCookies {
                        node_id,
                        url: url.to_string(),
                        batch: CookieImportBatch { cookies, skipped },
                    });
                }
                Err(message) => {
                    let _ = self
                        .sender
                        .send(BrowserEvent::ChromeAuthFailed { node_id, message });
                }
            }
            return;
        }
        if let Err(message) = request_chrome_cookies(node_id, url, self.sender.clone()) {
            tracing::warn!(node_id, %message, "Chrome Browser auth request could not start");
            let _ = self
                .sender
                .send(BrowserEvent::ChromeAuthFailed { node_id, message });
        }
    }

    pub(crate) fn fail_chrome_auth(&mut self, node_id: NodeId, message: &str) {
        tracing::warn!(node_id, error = %message, "Chrome Browser auth import failed");
        self.auth_states.insert(node_id, BrowserAuthUiState::Failed);
    }

    pub(crate) fn apply_chrome_auth(
        &mut self,
        node_id: NodeId,
        url: &str,
        batch: CookieImportBatch,
    ) -> Result<Option<(usize, usize)>, String> {
        if batch.cookies.is_empty() {
            return Err("Chrome returned no compatible cookies for this page".to_string());
        }
        let Some(view) = self.views.get_mut(&node_id) else {
            return Ok(None);
        };
        if view.url != url {
            return Ok(None);
        }
        let status = if self.native_views_enabled {
            view.import_cookies(node_id, url, &batch.cookies, batch.skipped)?
        } else {
            #[cfg(feature = "e2e-test-support")]
            {
                CookieImportStatus::Complete(batch.cookies.len())
            }
            #[cfg(not(feature = "e2e-test-support"))]
            {
                return Err("Browser native views were disabled during Cookie import".to_string());
            }
        };
        match status {
            CookieImportStatus::Complete(imported) => {
                view.reload();
                self.auth_states.insert(node_id, BrowserAuthUiState::Ready);
                Ok(Some((imported, batch.skipped)))
            }
            CookieImportStatus::Pending => Ok(None),
        }
    }

    pub(crate) fn complete_chrome_auth(&mut self, node_id: NodeId, url: &str) -> bool {
        let Some(view) = self.views.get_mut(&node_id) else {
            return false;
        };
        if view.url != url {
            return false;
        }
        view.reload();
        self.auth_states.insert(node_id, BrowserAuthUiState::Ready);
        true
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn set_chrome_auth_fake_result(
        &mut self,
        result: Option<Result<(usize, usize), String>>,
    ) {
        self.auth_fake_result = result;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn report_location_for_e2e(&self, node_id: NodeId, url: String) {
        let _ = self
            .sender
            .send(BrowserEvent::LocationChanged { node_id, url });
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

    fn current_url_for_polling(&self) -> Option<String> {
        self.native_view
            .as_ref()
            .and_then(NativeBrowserView::current_url_for_polling)
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
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if let Some(native_view) = &mut self.native_view {
            native_view.set_visible(visible);
        }
    }

    fn release_input(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.release_input();
        }
    }

    fn import_cookies(
        &mut self,
        node_id: NodeId,
        source_url: &str,
        cookies: &[super::chrome_auth::BrowserCookie],
        skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        self.native_view
            .as_mut()
            .ok_or_else(|| "Browser native view was unavailable for Cookie import".to_string())?
            .import_cookies(node_id, source_url, cookies, skipped)
    }
}
