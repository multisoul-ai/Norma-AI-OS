//! Pure desktop assistant drawer decisions; GPUI only applies the resulting view state.
//!
//! Floating overview geometry follows the assistant prototype: the panel is anchored
//! to the canvas top-right, then translated by an integer offset and clamped inside
//! the canvas. Dragging adds a delta to that offset with no animation.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DesktopLeftView {
    #[default]
    Chat,
    Overview,
}

/// Pixel offset from the floating panel's default top-right anchor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverviewOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DesktopDrawerState {
    pub left_open: bool,
    pub right_open: bool,
    pub left_view: DesktopLeftView,
    pub overview_floating: bool,
    pub overview_offset: OverviewOffset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopDrawerEvent {
    ToggleLeft,
    ToggleRight,
    ToggleRunOverview,
    SelectLeftView(DesktopLeftView),
    ToggleOverviewPin,
    ResetOverviewOffset,
    EscapeOverview,
    OpenArtifact,
    VoiceTriggered,
    ArtifactReady,
    ResizeLeft(bool),
    ResizeRight(bool),
}

/// Where Escape should land when the overview owns focus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverviewEscape {
    /// Focus is inside the floating panel, so it returns to the status display.
    ReturnFocusToStatus,
    /// Focus is inside the drawer overview, so the drawer returns to chat.
    ReturnDrawerToChat,
    Ignore,
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

/// Canvas rectangle the floating panel must stay inside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverviewCanvas {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverviewPlacement {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub max_height: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewLayoutPreference {
    pub left_view: DesktopLeftView,
    pub floating: bool,
    pub offset: OverviewOffset,
}

impl Default for OverviewLayoutPreference {
    fn default() -> Self {
        Self {
            left_view: DesktopLeftView::Chat,
            floating: false,
            offset: OverviewOffset::default(),
        }
    }
}

/// Prototype: `width: min(360px, 100% - 24px)`.
pub const OVERVIEW_PANEL_MAX_WIDTH: f32 = 360.0;
/// Prototype: `max-height: min(640px, 100% - 150px)`.
pub const OVERVIEW_PANEL_MAX_HEIGHT: f32 = 640.0;
pub const OVERVIEW_ANCHOR_TOP: f32 = 64.0;
pub const OVERVIEW_ANCHOR_RIGHT: f32 = 12.0;
pub const OVERVIEW_MARGIN: f32 = 8.0;
/// Top edge stays 4px inside the canvas so it does not cover the top.
pub const OVERVIEW_TOP_INSET: f32 = 4.0;
pub const OVERVIEW_NUDGE_STEP: i32 = 16;
pub const OVERVIEW_NUDGE_SHIFT_STEP: i32 = 48;

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
            if state.overview_floating {
                dock_overview(&mut state);
            } else {
                state.left_view = match state.left_view {
                    DesktopLeftView::Chat => DesktopLeftView::Overview,
                    DesktopLeftView::Overview => DesktopLeftView::Chat,
                };
                if state.left_view == DesktopLeftView::Overview {
                    state.left_open = true;
                }
            }
        }
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview)
            if state.overview_floating =>
        {
            dock_overview(&mut state);
        }
        DesktopDrawerEvent::SelectLeftView(view) => {
            state.left_view = view;
            if view == DesktopLeftView::Overview {
                state.left_open = true;
            }
        }
        DesktopDrawerEvent::ToggleOverviewPin => {
            if state.overview_floating {
                dock_overview(&mut state);
            } else {
                state.overview_floating = true;
                state.left_view = DesktopLeftView::Chat;
                state.left_open = true;
            }
        }
        DesktopDrawerEvent::ResetOverviewOffset => {
            state.overview_offset = OverviewOffset::default();
        }
        DesktopDrawerEvent::EscapeOverview => {
            if state.left_view == DesktopLeftView::Overview && !state.overview_floating {
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

fn dock_overview(state: &mut DesktopDrawerState) {
    state.overview_floating = false;
    state.overview_offset = OverviewOffset::default();
    state.left_view = DesktopLeftView::Overview;
    state.left_open = true;
}

/// #126 reads this to mark the status display with a pin while the overview floats.
pub fn status_display_marks_pin(state: &DesktopDrawerState) -> bool {
    state.overview_floating
}

pub fn overview_key_step(shift: bool) -> i32 {
    if shift {
        OVERVIEW_NUDGE_SHIFT_STEP
    } else {
        OVERVIEW_NUDGE_STEP
    }
}

pub fn route_overview_escape(
    mut state: DesktopDrawerState,
    focus_in_floating_panel: bool,
) -> (DesktopDrawerState, OverviewEscape) {
    if state.overview_floating && focus_in_floating_panel {
        return (state, OverviewEscape::ReturnFocusToStatus);
    }
    if state.left_view == DesktopLeftView::Overview && !state.overview_floating {
        state.left_view = DesktopLeftView::Chat;
        return (state, OverviewEscape::ReturnDrawerToChat);
    }
    (state, OverviewEscape::Ignore)
}

pub fn overview_panel_size(canvas: OverviewCanvas) -> (f32, f32) {
    let width = (canvas.width - 24.0).clamp(1.0, OVERVIEW_PANEL_MAX_WIDTH);
    let height = (canvas.height - 150.0).clamp(1.0, OVERVIEW_PANEL_MAX_HEIGHT);
    (width, height)
}

/// Move by a pointer or key delta, then keep the panel inside the canvas.
pub fn move_overview_offset(
    start: OverviewOffset,
    dx: f32,
    dy: f32,
    canvas: OverviewCanvas,
) -> OverviewOffset {
    clamp_overview_point(start.x as f32 + dx, start.y as f32 + dy, canvas)
}

pub fn clamp_overview_offset(offset: OverviewOffset, canvas: OverviewCanvas) -> OverviewOffset {
    clamp_overview_point(offset.x as f32, offset.y as f32, canvas)
}

fn clamp_overview_point(x: f32, y: f32, canvas: OverviewCanvas) -> OverviewOffset {
    let (width, height) = overview_panel_size(canvas);
    let base_left = canvas.width - OVERVIEW_ANCHOR_RIGHT - width;
    let min_x = OVERVIEW_MARGIN - base_left;
    let max_x = (canvas.width - OVERVIEW_MARGIN - width - base_left).max(min_x);
    let min_y = OVERVIEW_TOP_INSET - OVERVIEW_ANCHOR_TOP;
    let max_y = (canvas.height - OVERVIEW_MARGIN - height - OVERVIEW_ANCHOR_TOP).max(min_y);
    OverviewOffset {
        x: x.clamp(min_x, max_x).round() as i32,
        y: y.clamp(min_y, max_y).round() as i32,
    }
}

pub fn place_overview(offset: OverviewOffset, canvas: OverviewCanvas) -> OverviewPlacement {
    let clamped = clamp_overview_offset(offset, canvas);
    let (width, max_height) = overview_panel_size(canvas);
    let base_left = canvas.width - OVERVIEW_ANCHOR_RIGHT - width;
    OverviewPlacement {
        left: base_left + clamped.x as f32,
        top: OVERVIEW_ANCHOR_TOP + clamped.y as f32,
        width,
        max_height,
    }
}

pub fn left_view_preference_json(view: DesktopLeftView) -> &'static str {
    match view {
        DesktopLeftView::Chat => r#"{"left_view":"chat"}"#,
        DesktopLeftView::Overview => r#"{"left_view":"overview"}"#,
    }
}

pub fn left_view_from_preference(raw: Option<&str>) -> DesktopLeftView {
    overview_layout_from_preference(raw).left_view
}

pub fn overview_layout_json(state: &DesktopDrawerState) -> String {
    let view = match state.left_view {
        DesktopLeftView::Chat => "chat",
        DesktopLeftView::Overview => "overview",
    };
    format!(
        r#"{{"floating":{},"left_view":"{}","offset_x":{},"offset_y":{}}}"#,
        if state.overview_floating {
            "true"
        } else {
            "false"
        },
        view,
        state.overview_offset.x,
        state.overview_offset.y,
    )
}

pub fn overview_layout_from_preference(raw: Option<&str>) -> OverviewLayoutPreference {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return OverviewLayoutPreference::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return OverviewLayoutPreference::default();
    };
    let left_view = match value.get("left_view").and_then(|item| item.as_str()) {
        Some("overview") => DesktopLeftView::Overview,
        _ => DesktopLeftView::Chat,
    };
    let floating = value
        .get("floating")
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    let offset = OverviewOffset {
        x: json_i32(value.get("offset_x")),
        y: json_i32(value.get("offset_y")),
    };
    OverviewLayoutPreference {
        left_view,
        floating,
        offset,
    }
}

fn json_i32(value: Option<&serde_json::Value>) -> i32 {
    value
        .and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_f64().map(|number| number as i64))
        })
        .unwrap_or(0)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
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

/// Capsule click. A floating overview keeps its panel and only asks the UI to focus it.
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

/// Restore the saved view, floating flag, and offset. A canvas reclamps the offset.
pub fn restore_overview_layout(
    mut state: DesktopDrawerState,
    preference: OverviewLayoutPreference,
    canvas: Option<OverviewCanvas>,
) -> DesktopDrawerState {
    if preference.floating {
        state.overview_floating = true;
        state.left_view = DesktopLeftView::Chat;
        state.left_open = true;
        state.overview_offset = match canvas {
            Some(canvas) => clamp_overview_offset(preference.offset, canvas),
            None => preference.offset,
        };
        return state;
    }
    state.overview_floating = false;
    state.overview_offset = OverviewOffset::default();
    restore_left_view(state, preference.left_view)
}

#[cfg(test)]
#[path = "../tests/unit/desktop_drawers.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/status_capsule_click.rs"]
mod status_capsule_click;
