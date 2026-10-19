use super::*;
use gpui::MouseUpEvent;

pub(crate) fn on_mouse_up(
    app: &mut MultisoulApp,
    ev: &MouseUpEvent,
    w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    if app.interaction.finish_preview_pan() {
        app.interaction.last_motion_notify = None;
        cx.notify();
        return;
    }
    if let Some(pending) = app.interaction.take_pending_terminal_link() {
        let release_screen = pt_to_vec(ev.position);
        if terminal_link_dragged(pending.pressed_screen, release_screen) {
            if let Some(point) =
                terminal_selection_point_for_screen(app, pending.node_id, release_screen)
            {
                app.interaction
                    .begin_text_selection(pending.node_id, pending.anchor);
                app.interaction
                    .update_text_selection_head(pending.node_id, point);
                app.interaction.finish_text_selection();
            }
            cx.notify();
            return;
        }
        match pending.intent {
            TerminalLinkIntent::OpenBrowserCard { url } => {
                app.interaction.update_hovered_terminal_link(None);
                app.open_terminal_link_in_browser_card(url, w, cx)
            }
            TerminalLinkIntent::OpenDefaultBrowser { url } => {
                app.interaction.update_hovered_terminal_link(None);
                app.open_url_in_default_browser(url)
            }
            TerminalLinkIntent::OpenPath { paths, system } => {
                app.interaction.update_hovered_terminal_link(None);
                app.open_terminal_path(paths, system, pending.node_id, w, cx)
            }
            TerminalLinkIntent::InvalidLink
            | TerminalLinkIntent::BeginTextSelection
            | TerminalLinkIntent::Ignore => {}
        }
        cx.notify();
        return;
    }
    let presented_drag = app
        .interaction
        .drag
        .is_some_and(|drag| app.desktop_presentation_bounds.contains_key(&drag.node));
    let mut should_persist_canvas =
        app.interaction.panning || app.interaction.drag.is_some_and(|_| !presented_drag);
    let was_text_selecting = app
        .interaction
        .active_text_selection
        .map(|selection| selection.dragging)
        .unwrap_or(false);
    // Execute card swap before clearing drag state.
    if let Some(drag) = &app.interaction.drag {
        if let Some(target) = drag.swap_candidate {
            let dragged = drag.node;
            if app.runtime.document.swap_node_order(dragged, target) {
                should_persist_canvas = true;
                if app.desktop_host.is_none() {
                    if app.master_layout.is_active() {
                        app.apply_layout_for_current_mode(w);
                    } else {
                        app.reflow_tiled_nodes(w);
                    }
                }
            }
        }
    }
    app.interaction.panning = false;
    app.interaction.drag = None;
    app.interaction.last_screen = None;
    app.interaction.last_motion_notify = None;
    app.motion.clear_transient();
    if app.desktop_host.is_some() {
        app.refresh_desktop_presentation(w);
    }
    if was_text_selecting {
        app.interaction.finish_text_selection();
        cx.notify();
        return;
    }
    if should_persist_canvas {
        app.mark_canvas_dirty();
    }
    cx.notify();
}
