//! Pure right-drawer decisions; GPUI only applies the resulting view state.
//!
//! The dynamic island owns pending work, running agents and the staff; the only
//! workspace drawer left is the right-hand content area.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopDrawerState {
    pub right_open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopDrawerEvent {
    ToggleRight,
    OpenArtifact,
    VoiceTriggered,
    ArtifactReady,
    ResizeRight(bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopDrawerResize {
    pub width: f32,
    pub open: bool,
}

/// Resolve the right drawer's width from its inner (left) edge. A subminimum drag collapses it.
pub fn resize_drawer(
    initial_width: f32,
    initial_x: f32,
    current_x: f32,
    minimum_width: f32,
    maximum_width: f32,
) -> DesktopDrawerResize {
    let requested = initial_width - (current_x - initial_x);
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
        DesktopDrawerEvent::ToggleRight => state.right_open = !state.right_open,
        DesktopDrawerEvent::OpenArtifact => state.right_open = true,
        DesktopDrawerEvent::ResizeRight(open) => state.right_open = open,
        DesktopDrawerEvent::VoiceTriggered | DesktopDrawerEvent::ArtifactReady => {}
    }
    state
}

#[cfg(test)]
#[path = "../tests/unit/desktop_drawers.rs"]
mod tests;
