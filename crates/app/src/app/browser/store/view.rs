//! Lifecycle and commands for one native browser view.
use super::*;

impl BrowserView {
    pub(super) fn sync(
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
            let create_started = Instant::now();
            match NativeBrowserView::create(engine, node_id, url, bounds, window, event_sender) {
                Ok(native_view) => {
                    self.native_view = Some(native_view);
                    tracing::info!(
                        node_id,
                        create_ms = create_started.elapsed().as_secs_f64() * 1000.0,
                        "browser_view_created"
                    );
                    self.load_timeline
                        .begin(create_started, Some(create_started.elapsed()));
                }
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
            self.load_timeline.note_resize();
            self.set_bounds(bounds);
        }
        if self.url != url {
            self.url = url.to_string();
            self.load_url(url);
        }
        self.set_visible(true);
    }

    pub(super) fn load_url(&mut self, url: &str) {
        if let Some(native_view) = &mut self.native_view {
            self.load_timeline.begin(Instant::now(), None);
            native_view.load_url(url);
        }
    }

    pub(super) fn report_resource_timing(&self, node_id: NodeId) {
        if let Some(native_view) = &self.native_view {
            native_view.report_resource_timing(node_id);
        }
    }

    pub(super) fn current_url_for_polling(&self) -> Option<String> {
        self.native_view
            .as_ref()
            .and_then(NativeBrowserView::current_url_for_polling)
    }

    pub(super) fn go_back(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.go_back();
        }
    }

    pub(super) fn go_forward(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.go_forward();
        }
    }

    pub(super) fn reload(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            self.load_timeline.begin(Instant::now(), None);
            native_view.reload();
        }
    }

    pub(super) fn stop_loading(&mut self) {
        self.load_timeline.cancel();
        if let Some(native_view) = &self.native_view {
            native_view.stop_loading();
        }
    }

    pub(super) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        if let Some(native_view) = &mut self.native_view {
            native_view.set_bounds(bounds);
        }
    }

    pub(super) fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if let Some(native_view) = &mut self.native_view {
            native_view.set_visible(visible);
        }
    }

    pub(super) fn release_input(&mut self) {
        if let Some(native_view) = &mut self.native_view {
            native_view.release_input();
        }
    }

    pub(super) fn import_cookies(
        &mut self,
        node_id: NodeId,
        source_url: &str,
        cookies: &[super::super::chrome_auth::BrowserCookie],
        skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        self.native_view
            .as_mut()
            .ok_or_else(|| "Browser native view was unavailable for Cookie import".to_string())?
            .import_cookies(node_id, source_url, cookies, skipped)
    }
}
