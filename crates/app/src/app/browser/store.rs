//! Browser 原生视图的同步、可见性与输入焦点生命周期。

use super::{BrowserEngineKind, BrowserEvent, BrowserViewBounds, NativeBrowserView};
use canvas::NodeId;
use crossbeam_channel::{unbounded, Receiver, Sender};
use gpui::Window;
use std::collections::{HashMap, HashSet};

pub(crate) struct BrowserStore {
    views: HashMap<NodeId, BrowserView>,
    sender: Sender<BrowserEvent>,
    receiver: Receiver<BrowserEvent>,
    native_views_enabled: bool,
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
        if self.native_views_enabled {
            view.sync(node_id, url, bounds, window, event_sender);
        }
    }

    pub(crate) fn disable_native_views(&mut self) {
        self.native_views_enabled = false;
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
        self.release_input(node_id);
        if let Some(view) = self.views.get_mut(&node_id) {
            view.set_visible(false);
        }
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
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
}
