//! 画布节点渲染：viewport culling、标题栏、内容体。
//! 事件处理（pan、拖拽、单击）见 `events` 子模块。

use super::events;
use super::node_label::node_type_label;
use super::overlays::swap_highlight_overlay;
use super::run_effects::{self, RunEffectSync};
use super::working_snakes::{self, WorkingSnakeGeometry};
use crate::app::browser::native::BROWSER_NATIVE_BORDER_INSET_PX;
use crate::app::browser::{browser_native_view_bounds, native_browser_views_visible};
use crate::app::layout_menu::{browser_content_occluded_by_overlay, layout_menu_screen_rect};
use crate::app::MultisoulApp;
#[cfg(test)]
use crate::canvas::events::{should_focus_app_root_for_mouse_down, should_notify_motion_frame};
#[cfg(test)]
use crate::canvas::geometry::terminal_content_local_position;
#[cfg(test)]
use crate::canvas::geometry::{apply_canvas_wheel_scroll, click_event_for_node, is_title_bar_hit};
use crate::canvas::geometry::{node_card_radius_px, node_content_radius_px};
#[cfg(test)]
use crate::canvas::interaction::InteractionState;
use crate::cards::agent::queue::{
    render_badge as render_agent_queue_badge, waiting_count as agent_waiting_count,
};
use crate::cards::border_glow::render_border_glow;
use crate::cards::browser::card::BROWSER_TOOLBAR_H_PX;
use crate::cards::chrome::{
    card_background, card_title_background, node_shadow, render_card_title, render_task_badge,
    TITLE_BAR_H,
};
use crate::cards::{card_for, CardBodyContext};
#[cfg(test)]
use canvas::Vec2;
use canvas::{CanvasNodeKind, CanvasRect, NodeId, NodeType};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, FontWeight, MouseButton, Window};
use runtime::InputMode;
#[cfg(test)]
use runtime::{route, Route};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use ui::icons::with_alpha;
static CANVAS_OPEN_RENDER_PENDING: AtomicBool = AtomicBool::new(false);

pub(crate) fn embedded_node_visible(node_type: NodeType, embedded_desktop: bool) -> bool {
    !embedded_desktop || matches!(node_type, NodeType::Agent | NodeType::Term)
}

pub(crate) fn mark_canvas_open_render_pending() {
    CANVAS_OPEN_RENDER_PENDING.store(true, Ordering::Release);
}

pub fn render_canvas(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    #[cfg(feature = "e2e-test-support")]
    {
        app.canvas_render_count_for_e2e += 1;
    }
    let render_start = Instant::now();
    let total_nodes = app.runtime.document.nodes().len();
    let _span = tracing::trace_span!("render_canvas", nodes = total_nodes).entered();

    let theme = app.theme;
    let vp = app.runtime.document.viewport;
    let win = window.viewport_size();
    let canvas_size = if app.desktop_host.is_some() {
        app.desktop_embedded_canvas_size.unwrap_or_else(|| {
            crate::workspace::desktop::desktop_scene_rect(
                f32::from(win.width),
                f32::from(win.height),
            )
            .size
        })
    } else {
        canvas::Vec2::new(f32::from(win.width), f32::from(win.height))
    };
    let (win_w, win_h) = (canvas_size.x, canvas_size.y);
    let focus_mode = app.runtime.focus.mode();
    let swap_candidate = app.interaction.drag.as_ref().and_then(|d| d.swap_candidate);
    let hovered_terminal_link = app.interaction.hovered_terminal_link.clone();
    let mut node_elems: Vec<AnyElement> = Vec::new();
    let native_browser_views_visible = native_browser_views_visible(app.settings_panel.open);
    // 底部弹出菜单打开时计算屏幕矩形；关闭时为 None（不做遮挡过滤）。
    let layout_menu_rect = layout_menu_screen_rect(app.interaction.layout_menu_open, win_w, win_h);
    let command_menu_rect = app.command_panel_rect(win_w, win_h);
    let mut visible_nodes = 0usize;
    let mut snake_nodes = 0usize;
    let node_ids: Vec<NodeId> = app.runtime.document.nodes().iter().map(|n| n.id).collect();
    app.motion.retain_nodes(node_ids.iter().copied());
    let mut snake_view_set_changed = working_snakes::retain_live_views(app, &node_ids);
    snake_view_set_changed |= run_effects::retain_live_views(app, &node_ids);
    let run_style = crate::app::run_effects_actions::run_style(cx);
    let run_reduce = run_effects::reduce_motion(app, run_style);
    for id in node_ids {
        let Some((canonical_rect, selected, node_type)) =
            app.runtime.document.node(id).map(|node| {
                (
                    app.presented_node_bounds(id).unwrap_or(node.bounds),
                    node.selected || app.runtime.selection.contains(id),
                    node.kind.node_type(),
                )
            })
        else {
            continue;
        };
        if !embedded_node_visible(node_type, app.desktop_host.is_some()) {
            continue;
        }
        let focused = matches!(focus_mode, InputMode::NodeFocused(fid) if fid == id);
        let is_swap_candidate = swap_candidate == Some(id);
        app.motion.ensure_node(id, canonical_rect);
        app.motion.set_selection_focus(id, selected, focused);
        let is_agent = node_type == NodeType::Agent;
        let agent_state = is_agent.then(|| app.runtime.agent_task_state(id)).flatten();
        let card_attention = app.runtime.card_attention(id);
        let (chase_active, breathing_intent) =
            run_effects::classic_intents(run_style, is_agent, agent_state.clone(), card_attention);
        app.motion.set_breathing_intent(id, breathing_intent);
        if app
            .interaction
            .drag
            .map(|drag| drag.node == id)
            .unwrap_or(false)
        {
            app.motion.sync_drag_rect(id, canonical_rect);
        }

        let visual = app
            .motion
            .resolve_node_visual_at(id, canonical_rect, theme, render_start);
        let top_left = vp.canvas_to_screen(visual.rect.origin);
        let (w, h) = (visual.rect.size.x * vp.zoom, visual.rect.size.y * vp.zoom);
        let node_radius = node_card_radius_px(vp.zoom);
        let visible = !(top_left.x + w < 0.0
            || top_left.y + h < 0.0
            || top_left.x > win_w
            || top_left.y > win_h);
        if is_agent {
            let geometry = WorkingSnakeGeometry {
                visible,
                left_px: top_left.x,
                top_px: top_left.y,
                width_px: w,
                height_px: h,
                opacity: visual.opacity,
                zoom: vp.zoom,
                node_radius_px: node_radius,
            };
            snake_view_set_changed |=
                working_snakes::synchronize_view(app, id, chase_active, theme, geometry, cx);
            let sync = RunEffectSync {
                style: run_style,
                state: agent_state.as_ref(),
                attention: card_attention,
                theme,
                geometry,
                reduce_motion: run_reduce,
            };
            snake_view_set_changed |= run_effects::synchronize_view(app, id, sync, cx);
        }

        if !visible {
            continue;
        }
        visible_nodes += 1;

        let node = app.runtime.document.node(id).unwrap().clone();
        let card = card_for(node_type);
        let chrome = card.chrome(&node, &app.runtime);
        let queue_count = if is_agent {
            agent_waiting_count(app, id)
        } else {
            0
        };
        let initialization = app.motion.initialization_presentation(id);
        let title = chrome.title;
        let title_detail = chrome.title_detail;
        let task_badge = chrome.task_badge;
        let refreshable = chrome.refreshable;
        let stroke = visual.stroke_color;
        let status = visual.status_color;
        let close_button_size = (18.0 * vp.zoom).clamp(16.0, 24.0);
        let border_width = if visual.border_is_emphasized {
            2.0
        } else {
            1.0
        };
        let content_radius = node_content_radius_px(vp.zoom, border_width);

        let mut node_div = div()
            .id(("node-card", id))
            .absolute()
            .left(px(top_left.x))
            .top(px(top_left.y))
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .shadow(node_shadow(visual))
            .border(px(border_width))
            .border_color(rgba(stroke))
            .opacity(visual.opacity)
            .rounded(px(node_radius))
            .bg(card_background(theme))
            .overflow_hidden()
            .on_hover(cx.listener(move |app, hovered: &bool, _window, cx| {
                app.motion.set_hover(id, *hovered);
                cx.notify();
            }));
        if is_agent {
            node_div = node_div.debug_selector(|| "agent-card".to_string());
        }

        if hovered_terminal_link
            .as_ref()
            .map(|link| link.node_id == id)
            .unwrap_or(false)
        {
            node_div = node_div.cursor_pointer();
        }

        // 标题栏
        let mut title_actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0 * vp.zoom));
        if refreshable {
            title_actions = title_actions.child(
                div()
                    .id(("node-refresh", id))
                    .w(px(close_button_size))
                    .h(px(close_button_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px((6.0 * vp.zoom).clamp(5.0, 8.0)))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x12)))
                    .text_color(rgba(theme.text_tertiary))
                    .text_size(px((12.0 * vp.zoom).clamp(10.0, 13.0)))
                    .child("R")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _window, cx| {
                        cx.stop_propagation();
                        app.refresh_preview_node(id, cx);
                        cx.notify();
                    })),
            );
        }
        title_actions = title_actions.child(
            div()
                .id(("node-close", id))
                .debug_selector(|| "node-close".to_string())
                .w(px(close_button_size))
                .h(px(close_button_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px((6.0 * vp.zoom).clamp(5.0, 8.0)))
                .bg(rgba(with_alpha(theme.destructive, 0x12)))
                .text_color(rgba(theme.text_tertiary))
                .text_size(px((12.0 * vp.zoom).clamp(10.0, 13.0)))
                .child("×")
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.close_node_through_route(id, window);
                    cx.notify();
                })),
        );

        let title_bar = div()
            .debug_selector(|| "card-titlebar".to_string())
            .h(px(TITLE_BAR_H * vp.zoom))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .rounded_t(px(content_radius))
            .border_b_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x70)))
            .bg(card_title_background(visual.title_bar_color, theme))
            .text_color(rgba(theme.text_primary))
            .text_size(px((12.0 * vp.zoom).max(9.0)))
            .px(px(10.0 * vp.zoom))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .gap_2()
                    .child(
                        div()
                            .h(px((20.0 * vp.zoom).clamp(18.0, 22.0)))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(5.0 * vp.zoom))
                            .px(px(7.0 * vp.zoom))
                            .rounded(px((10.0 * vp.zoom).clamp(9.0, 11.0)))
                            .bg(rgba(with_alpha(status, 0x18)))
                            .text_color(rgba(status))
                            .text_size(px((9.0 * vp.zoom).clamp(8.0, 10.0)))
                            .font_weight(FontWeight::BOLD)
                            .child(
                                div()
                                    .w(px(6.0 * vp.zoom))
                                    .h(px(6.0 * vp.zoom))
                                    .rounded(px(3.0 * vp.zoom))
                                    .bg(rgba(status)),
                            )
                            .child(node_type_label(node_type)),
                    )
                    .child(render_card_title(id, title, title_detail, vp.zoom, theme))
                    .when_some(task_badge, |this, badge| {
                        this.child(render_task_badge(badge, vp.zoom, theme))
                    })
                    .when(queue_count > 0, |this| {
                        this.child(render_agent_queue_badge(
                            id,
                            queue_count,
                            vp.zoom,
                            theme,
                            cx,
                        ))
                    }),
            )
            .child(title_actions);
        let title_layer = div()
            .debug_selector(|| "card-title-layer".to_string())
            .h(px(TITLE_BAR_H * vp.zoom))
            .w_full()
            .flex_none()
            .relative()
            .child(title_bar);

        let preview_image_viewer = app.previews.image_viewer_presentation(id);
        let preview_image_dragging = app
            .interaction
            .preview_pan
            .is_some_and(|pan| pan.node_id == id);
        // 浏览器卡片：计算其内容区是否被底部弹出菜单遮挡。
        let occluded_by_overlay = matches!(node_type, NodeType::Browser) && {
            let browser_header_h = (TITLE_BAR_H + BROWSER_TOOLBAR_H_PX) * vp.zoom;
            let content_rect = CanvasRect::new(
                top_left.x + BROWSER_NATIVE_BORDER_INSET_PX as f32,
                top_left.y + browser_header_h + BROWSER_NATIVE_BORDER_INSET_PX as f32,
                (w - BROWSER_NATIVE_BORDER_INSET_PX as f32 * 2.0).max(0.0),
                (h - browser_header_h - BROWSER_NATIVE_BORDER_INSET_PX as f32 * 2.0).max(0.0),
            );
            browser_content_occluded_by_overlay(content_rect, layout_menu_rect)
                || browser_content_occluded_by_overlay(content_rect, command_menu_rect)
        };
        let card_body = card.body(
            &node,
            CardBodyContext {
                runtime: &app.runtime,
                terminal_render_cache: app.terminal_render_cache.clone(),
                terminal_path_cache: app.terminal_path_cache.clone(),
                preview_state: app.previews.state_for(id),
                preview_editor: app.previews.editor_for(id),
                preview_image_viewer,
                preview_image_dragging,
                text_selection: app.interaction.active_text_selection,
                focused,
                reduce_motion: app.motion.reduce_motion,
                initialization,
                terminal_cursor_phase: app.terminal_cursor_phase,
                browser_address_input: app.browser_address_input_for(id),
                browser_auth_state: app.browser_chrome_auth_state_for(id),
                browser_loading: app.browsers.is_loading(id),
                ego_preview: app.ego_previews.frame_for(id),
                #[cfg(all(target_os = "macos", not(feature = "e2e-test-support")))]
                ego_live_preview: app.ego_previews.live_frame_for(id),
                ego_preview_error: app.ego_previews.error_for(id),
                occluded_by_overlay,
                theme,
                zoom: vp.zoom,
                bottom_radius_px: content_radius,
            },
        );
        if matches!(node_type, NodeType::Browser) && native_browser_views_visible {
            if let CanvasNodeKind::Browser(spec) = &node.kind {
                if matches!(&spec.surface, canvas::BrowserSurfaceKind::Native)
                    && !occluded_by_overlay
                {
                    let browser_header_h = (TITLE_BAR_H + BROWSER_TOOLBAR_H_PX) * vp.zoom;
                    node_elems.push(app.browser_scene_element(
                        id,
                        spec.url.clone(),
                        browser_native_view_bounds(
                            f64::from(top_left.x),
                            f64::from(top_left.y),
                            f64::from(w),
                            f64::from(h),
                            f64::from(browser_header_h),
                            f64::from(node_content_radius_px(
                                vp.zoom,
                                BROWSER_NATIVE_BORDER_INSET_PX as f32,
                            )),
                        ),
                        cx,
                    ));
                }
            }
        }
        node_elems.extend(render_border_glow(
            CanvasRect::new(top_left.x, top_left.y, w, h),
            node_radius,
            vp.zoom,
            visual,
        ));
        node_div = node_div.child(title_layer).child(card_body);
        if is_agent && chase_active {
            snake_nodes += 1;
        }
        node_elems.push(node_div.into_any_element());
        if is_swap_candidate {
            node_elems.push(swap_highlight_overlay(
                top_left.x,
                top_left.y,
                w,
                h,
                node_radius,
                theme.accent_violet,
            ));
        }
    }

    if CANVAS_OPEN_RENDER_PENDING.swap(false, Ordering::AcqRel) {
        tracing::info!(
            stage = "workspace_open_first_canvas_render",
            total_nodes,
            visible_nodes,
            element_count = node_elems.len(),
            zoom = vp.zoom,
            window_width = win_w,
            window_height = win_h,
            elapsed_ms = render_start.elapsed().as_secs_f64() * 1000.0,
            "workspace_open_timing"
        );
    }
    crate::frame_diagnostics::CanvasRenderReport {
        total_nodes,
        visible_nodes,
        snake_nodes,
        element_count: node_elems.len(),
        zoom: vp.zoom,
        window_width: win_w,
        window_height: win_h,
        elapsed: render_start.elapsed(),
    }
    .log_if_enabled();
    if snake_view_set_changed {
        // Snake Entity 是 CanvasSurface 的兄弟叠层；集合变化时只需让根视图重新挂载
        // 一次。稳定动画帧只 notify Snake Entity，不会触发 CanvasSurface observer。
        cx.notify();
    }

    div()
        .id("canvas")
        .debug_selector(|| "canvas".to_string())
        .size_full()
        .relative()
        .on_scroll_wheel(cx.listener(events::on_scroll))
        .on_mouse_down(MouseButton::Left, cx.listener(events::on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(events::on_mouse_down))
        .on_mouse_move(cx.listener(events::on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(events::on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(events::on_mouse_up))
        .children(node_elems)
}

#[cfg(test)]
#[path = "../../tests/unit/chase_canvas_view_contract_tests.rs"]
mod chase_canvas_view_contract_tests;
#[cfg(test)]
#[path = "../../tests/unit/product_ui_canvas_tests.rs"]
mod product_ui_canvas_tests;
#[cfg(test)]
#[path = "../../tests/unit/canvas_view_tests.rs"]
mod tests;
