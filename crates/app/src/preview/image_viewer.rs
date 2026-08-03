use super::PreviewStore;
use canvas::{NodeId, Vec2};
use gpui::{point, px, Action, ScrollHandle};

const FIT_ZOOM: f32 = 1.0;
const ZOOM_STEP: f32 = 0.25;
const MAX_ZOOM: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageViewerState {
    image_count: usize,
    index: usize,
    zoom: f32,
}

impl ImageViewerState {
    pub(crate) fn new(image_count: usize) -> Self {
        Self {
            image_count,
            index: 0,
            zoom: FIT_ZOOM,
        }
    }

    pub(crate) fn index(self) -> usize {
        self.index
    }

    pub(crate) fn zoom(self) -> f32 {
        self.zoom
    }

    pub(crate) fn can_go_previous(self) -> bool {
        self.index > 0
    }

    pub(crate) fn can_go_next(self) -> bool {
        self.index.saturating_add(1) < self.image_count
    }

    pub(crate) fn previous(&mut self) -> bool {
        if !self.can_go_previous() {
            return false;
        }
        self.index = self.index.saturating_sub(1);
        self.fit();
        true
    }

    pub(crate) fn next(&mut self) -> bool {
        if !self.can_go_next() {
            return false;
        }
        self.index = self.index.saturating_add(1);
        self.fit();
        true
    }

    pub(crate) fn zoom_in(&mut self) -> bool {
        let zoom = (self.zoom + ZOOM_STEP).min(MAX_ZOOM);
        if zoom == self.zoom {
            return false;
        }
        self.zoom = zoom;
        true
    }

    pub(crate) fn zoom_out(&mut self) -> bool {
        let zoom = (self.zoom - ZOOM_STEP).max(FIT_ZOOM);
        if zoom == self.zoom {
            return false;
        }
        self.zoom = zoom;
        true
    }

    pub(crate) fn fit(&mut self) -> bool {
        if self.zoom == FIT_ZOOM {
            return false;
        }
        self.zoom = FIT_ZOOM;
        true
    }

    fn reconcile(&mut self, image_count: usize) {
        self.image_count = image_count;
        if self.index >= image_count {
            self.index = 0;
            self.zoom = FIT_ZOOM;
        }
    }
}

pub(crate) fn drag_scroll_offset(
    current: (f32, f32),
    pointer_delta: (f32, f32),
    max_offset: (f32, f32),
) -> (f32, f32) {
    (
        (current.0 + pointer_delta.0).clamp(-max_offset.0, 0.0),
        (current.1 + pointer_delta.1).clamp(-max_offset.1, 0.0),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PreviewImageCommandKind {
    Previous,
    Next,
    ZoomIn,
    ZoomOut,
    Fit,
    BeginPan { screen_x: f32, screen_y: f32 },
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = multisoul, no_json)]
pub(crate) struct PreviewImageCommand {
    pub(crate) node_id: NodeId,
    pub(crate) command: PreviewImageCommandKind,
}

#[derive(Clone)]
pub(crate) struct ImageViewerPresentation {
    pub(crate) index: usize,
    pub(crate) image_count: usize,
    pub(crate) zoom: f32,
    pub(crate) can_go_previous: bool,
    pub(crate) can_go_next: bool,
    pub(crate) can_zoom_out: bool,
    pub(crate) scroll_handle: ScrollHandle,
}

pub(super) struct ImageViewerSession {
    state: ImageViewerState,
    scroll_handle: ScrollHandle,
}

impl ImageViewerSession {
    fn new(image_count: usize) -> Self {
        Self {
            state: ImageViewerState::new(image_count),
            scroll_handle: ScrollHandle::new(),
        }
    }

    fn presentation(&self) -> ImageViewerPresentation {
        ImageViewerPresentation {
            index: self.state.index(),
            image_count: self.state.image_count,
            zoom: self.state.zoom(),
            can_go_previous: self.state.can_go_previous(),
            can_go_next: self.state.can_go_next(),
            can_zoom_out: self.state.zoom() > FIT_ZOOM,
            scroll_handle: self.scroll_handle.clone(),
        }
    }

    fn apply(&mut self, command: PreviewImageCommandKind) -> bool {
        let old_zoom = self.state.zoom();
        let changed = match command {
            PreviewImageCommandKind::Previous => self.state.previous(),
            PreviewImageCommandKind::Next => self.state.next(),
            PreviewImageCommandKind::ZoomIn => self.state.zoom_in(),
            PreviewImageCommandKind::ZoomOut => self.state.zoom_out(),
            PreviewImageCommandKind::Fit => self.state.fit(),
            PreviewImageCommandKind::BeginPan { .. } => false,
        };
        if !changed {
            return false;
        }

        match command {
            PreviewImageCommandKind::ZoomIn | PreviewImageCommandKind::ZoomOut => {
                center_scroll_for_zoom(&self.scroll_handle, old_zoom, self.state.zoom());
            }
            PreviewImageCommandKind::Previous
            | PreviewImageCommandKind::Next
            | PreviewImageCommandKind::Fit => reset_scroll(&self.scroll_handle),
            PreviewImageCommandKind::BeginPan { .. } => {}
        }
        true
    }
}

impl PreviewStore {
    pub(crate) fn reconcile_image_viewer(&mut self, node_id: NodeId, image_count: Option<usize>) {
        let Some(image_count) = image_count.filter(|count| *count > 0) else {
            self.image_viewers.remove(&node_id);
            return;
        };
        self.image_viewers
            .entry(node_id)
            .and_modify(|session| session.state.reconcile(image_count))
            .or_insert_with(|| ImageViewerSession::new(image_count));
    }

    pub(crate) fn image_viewer_presentation(
        &self,
        node_id: NodeId,
    ) -> Option<ImageViewerPresentation> {
        self.image_viewers
            .get(&node_id)
            .map(ImageViewerSession::presentation)
    }

    pub(crate) fn apply_image_command(
        &mut self,
        node_id: NodeId,
        command: PreviewImageCommandKind,
    ) -> bool {
        self.image_viewers
            .get_mut(&node_id)
            .map(|session| session.apply(command))
            .unwrap_or(false)
    }

    pub(crate) fn image_can_pan(&self, node_id: NodeId) -> bool {
        self.image_viewers
            .get(&node_id)
            .map(|session| {
                let max_offset = session.scroll_handle.max_offset();
                session.state.zoom() > FIT_ZOOM
                    && (f32::from(max_offset.width) > 0.0 || f32::from(max_offset.height) > 0.0)
            })
            .unwrap_or(false)
    }

    pub(crate) fn pan_image(&mut self, node_id: NodeId, pointer_delta: Vec2) -> bool {
        let Some(session) = self.image_viewers.get(&node_id) else {
            return false;
        };
        let current = session.scroll_handle.offset();
        let max_offset = session.scroll_handle.max_offset();
        let next = drag_scroll_offset(
            (f32::from(current.x), f32::from(current.y)),
            (pointer_delta.x, pointer_delta.y),
            (f32::from(max_offset.width), f32::from(max_offset.height)),
        );
        if next == (f32::from(current.x), f32::from(current.y)) {
            return false;
        }
        session
            .scroll_handle
            .set_offset(point(px(next.0), px(next.1)));
        true
    }
}

fn center_scroll_for_zoom(scroll_handle: &ScrollHandle, old_zoom: f32, new_zoom: f32) {
    let ratio = new_zoom / old_zoom;
    let bounds = scroll_handle.bounds();
    let offset = scroll_handle.offset();
    let center_x = f32::from(bounds.size.width) / 2.0;
    let center_y = f32::from(bounds.size.height) / 2.0;
    let next_x = center_x - (center_x - f32::from(offset.x)) * ratio;
    let next_y = center_y - (center_y - f32::from(offset.y)) * ratio;
    scroll_handle.set_offset(point(px(next_x), px(next_y)));
}

fn reset_scroll(scroll_handle: &ScrollHandle) {
    scroll_handle.set_offset(point(px(0.0), px(0.0)));
}
