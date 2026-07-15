//! Full-width workspace title bar aligned with the reviewed Product UI canvas chrome.

use crate::app::MultisoulApp;
use crate::workspace::persistence::PersistenceSaveState;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, Context, Div, Stateful, WindowControlArea,
};
use ui::component::StyledExt;
use workspace::compact_path;

pub(super) const TOP_BAR_HEIGHT: f32 = 54.0;
pub(super) const TOP_BAR_FILL: u32 = 0x0d131f4d;
pub(super) const TOP_BAR_BORDER: u32 = 0xffffff29;
const TOP_BAR_FOREGROUND: u32 = 0xffffffe6;
const TOP_BAR_MUTED: u32 = 0xffffff7a;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CanvasSaveVisual {
    pub(super) label: &'static str,
    pub(super) dot: u32,
}

pub(super) fn canvas_save_visual(pending: bool, failed: bool) -> CanvasSaveVisual {
    if failed {
        CanvasSaveVisual {
            label: "Save issue",
            dot: 0xff6b7aff,
        }
    } else if pending {
        CanvasSaveVisual {
            label: "Saving…",
            dot: 0xd6ff3bff,
        }
    } else {
        CanvasSaveVisual {
            label: "Canvas saved",
            dot: 0x64e0b5ff,
        }
    }
}

impl MultisoulApp {
    pub(super) fn render_workspace_top_bar(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let project_path = self
            .workspace_state
            .current_root
            .as_deref()
            .map(compact_path)
            .unwrap_or_default();
        let save = match self.persistence.save_state() {
            PersistenceSaveState::Clean => canvas_save_visual(false, false),
            PersistenceSaveState::Pending => canvas_save_visual(true, false),
            PersistenceSaveState::Failed => canvas_save_visual(false, true),
        };

        div()
            .id("workspace-top-bar")
            .debug_selector(|| "workspace-top-bar".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(TOP_BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(rgba(TOP_BAR_BORDER))
            .bg(rgba(TOP_BAR_FILL))
            .shadow(top_bar_shadow())
            .window_control_area(WindowControlArea::Drag)
            .child(workspace_title(project_path))
            .child(self.workspace_top_bar_actions(save, cx))
    }

    fn workspace_top_bar_actions(&mut self, save: CanvasSaveVisual, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .top(px(0.0))
            .right(px(18.0))
            .h(px(TOP_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                launchpad_button().on_click(cx.listener(|_app, _, _window, cx| {
                    MultisoulApp::open_new_launchpad_window(cx);
                })),
            )
            .child(save_status(save))
    }
}

fn workspace_title(path: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .child(
            div()
                .font_semibold()
                .text_color(rgba(TOP_BAR_FOREGROUND))
                .child("Norma OS"),
        )
        .child(
            div()
                .max_w(px(360.0))
                .overflow_hidden()
                .truncate()
                .font_family("Menlo")
                .text_size(px(9.0))
                .text_color(rgba(TOP_BAR_MUTED))
                .child(path),
        )
}

fn launchpad_button() -> Stateful<Div> {
    div()
        .id("workspace-launchpad-button")
        .debug_selector(|| "workspace-launchpad-button".to_string())
        .w(px(38.0))
        .h(px(38.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(0xffffff2e))
        .bg(rgba(0xffffff1a))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff29)))
        .active(|style| style.bg(rgba(0xffffff38)))
        .child(launchpad_grid_glyph())
}

fn launchpad_grid_glyph() -> Div {
    div()
        .grid()
        .grid_cols(3)
        .grid_rows(3)
        .gap(px(2.0))
        .children((0..9).map(|_| {
            div()
                .w(px(2.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(rgba(0xffffffd6))
        }))
}

fn save_status(save: CanvasSaveVisual) -> Stateful<Div> {
    div()
        .id("workspace-save-status")
        .debug_selector(|| "workspace-save-status".to_string())
        .h(px(28.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .px(px(10.0))
        .rounded(px(14.0))
        .bg(rgba(0xffffff1f))
        .text_color(rgba(0xffffffbd))
        .text_size(px(10.0))
        .font_semibold()
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgba(save.dot)),
        )
        .child(save.label)
}

fn top_bar_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x050a1414).into(),
        offset: point(px(0.0), px(8.0)),
        blur_radius: px(30.0),
        spread_radius: px(0.0),
    }]
}
