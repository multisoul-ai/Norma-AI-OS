//! Pure desktop assistant drawer decisions; GPUI only applies the resulting view state.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopDrawerState {
    pub left_open: bool,
    pub right_open: bool,
    pub overview_expanded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopDrawerEvent {
    ToggleLeft,
    ToggleRight,
    ToggleOverview,
    OpenArtifact,
    VoiceTriggered,
    ArtifactReady,
    ResizeLeft(bool),
    ResizeRight(bool),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopDrawerSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopDrawerResize {
    pub width: f32,
    pub open: bool,
}

/// Resolve the drawer's width from its inner edge. A subminimum drag collapses it.
pub fn resize_drawer(
    side: DesktopDrawerSide,
    initial_width: f32,
    initial_x: f32,
    current_x: f32,
    minimum_width: f32,
    maximum_width: f32,
) -> DesktopDrawerResize {
    let delta = current_x - initial_x;
    let requested = initial_width
        + match side {
            DesktopDrawerSide::Left => delta,
            DesktopDrawerSide::Right => -delta,
        };
    DesktopDrawerResize {
        width: requested.clamp(minimum_width, maximum_width.max(minimum_width)),
        open: requested >= minimum_width,
    }
}

pub fn route_drawer(
    mut state: DesktopDrawerState,
    event: DesktopDrawerEvent,
) -> DesktopDrawerState {
    match event {
        DesktopDrawerEvent::ToggleLeft => state.left_open = !state.left_open,
        DesktopDrawerEvent::ToggleRight => state.right_open = !state.right_open,
        DesktopDrawerEvent::ToggleOverview => state.overview_expanded = !state.overview_expanded,
        DesktopDrawerEvent::OpenArtifact => state.right_open = true,
        DesktopDrawerEvent::ResizeLeft(open) => state.left_open = open,
        DesktopDrawerEvent::ResizeRight(open) => state.right_open = open,
        DesktopDrawerEvent::VoiceTriggered | DesktopDrawerEvent::ArtifactReady => {}
    }
    state
}

#[cfg(test)]
#[path = "../tests/unit/desktop_drawers.rs"]
mod tests;
