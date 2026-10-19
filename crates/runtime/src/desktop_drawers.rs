//! Pure desktop assistant drawer decisions; GPUI only applies the resulting view state.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DesktopLeftView {
    #[default]
    Chat,
    Overview,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopDrawerState {
    pub left_open: bool,
    pub right_open: bool,
    pub left_view: DesktopLeftView,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopDrawerEvent {
    ToggleLeft,
    ToggleRight,
    ToggleRunOverview,
    SelectLeftView(DesktopLeftView),
    EscapeOverview,
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
        DesktopDrawerEvent::ToggleRunOverview => {
            state.left_view = match state.left_view {
                DesktopLeftView::Chat => DesktopLeftView::Overview,
                DesktopLeftView::Overview => DesktopLeftView::Chat,
            };
            if state.left_view == DesktopLeftView::Overview {
                state.left_open = true;
            }
        }
        DesktopDrawerEvent::SelectLeftView(view) => {
            state.left_view = view;
            if view == DesktopLeftView::Overview {
                state.left_open = true;
            }
        }
        DesktopDrawerEvent::EscapeOverview => {
            if state.left_view == DesktopLeftView::Overview {
                state.left_view = DesktopLeftView::Chat;
            }
        }
        DesktopDrawerEvent::OpenArtifact => state.right_open = true,
        DesktopDrawerEvent::ResizeLeft(open) => state.left_open = open,
        DesktopDrawerEvent::ResizeRight(open) => state.right_open = open,
        DesktopDrawerEvent::VoiceTriggered | DesktopDrawerEvent::ArtifactReady => {}
    }
    state
}

pub fn left_view_preference_json(view: DesktopLeftView) -> &'static str {
    match view {
        DesktopLeftView::Chat => r#"{"left_view":"chat"}"#,
        DesktopLeftView::Overview => r#"{"left_view":"overview"}"#,
    }
}

pub fn left_view_from_preference(raw: Option<&str>) -> DesktopLeftView {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return DesktopLeftView::Chat;
    };
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => match value.get("left_view").and_then(|item| item.as_str()) {
            Some("overview") => DesktopLeftView::Overview,
            _ => DesktopLeftView::Chat,
        },
        Err(_) => DesktopLeftView::Chat,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusCapsuleClick {
    pub floating: bool,
    pub from_keyboard: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusCapsuleEffect {
    FocusFloating,
    ShowOverview { focus_first_row: bool },
    ReturnToChat,
}

/// Capsule click. Floating focus is a branch for the later floating panel; this state has no pin.
pub fn route_status_capsule(
    mut state: DesktopDrawerState,
    click: StatusCapsuleClick,
) -> (DesktopDrawerState, StatusCapsuleEffect) {
    if click.floating {
        return (state, StatusCapsuleEffect::FocusFloating);
    }
    if state.left_open && state.left_view == DesktopLeftView::Overview {
        state.left_view = DesktopLeftView::Chat;
        return (state, StatusCapsuleEffect::ReturnToChat);
    }
    state.left_open = true;
    state.left_view = DesktopLeftView::Overview;
    (
        state,
        StatusCapsuleEffect::ShowOverview {
            focus_first_row: click.from_keyboard,
        },
    )
}

/// Restoring an overview view also opens a collapsed left drawer so the view is visible.
pub fn restore_left_view(
    mut state: DesktopDrawerState,
    view: DesktopLeftView,
) -> DesktopDrawerState {
    state.left_view = view;
    if view == DesktopLeftView::Overview {
        state.left_open = true;
    }
    state
}

#[cfg(test)]
#[path = "../tests/unit/desktop_drawers.rs"]
mod tests;
