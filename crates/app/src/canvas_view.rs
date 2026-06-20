//! 画布渲染与交互：pan（空格/中键拖）、zoom（滚轮，绕光标）、节点拖拽、单击选中。
//! viewport culling：屏幕外节点不构建元素。

use crate::app::MultisoulApp;
use crate::terminal_view::render_terminal_node;
use canvas::{hit_test, CanvasNodeKind, NodeId, Vec2};
use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent,
    Window,
};
use runtime::{route, InputEvent, InputMode, Route};
use ui::Theme;

#[derive(Clone, Copy)]
pub struct DragState {
    pub node: NodeId,
    pub last_screen: Vec2,
}

#[derive(Default)]
pub struct InteractionState {
    pub space_held: bool,
    pub panning: bool,
    pub last_screen: Option<Vec2>,
    pub drag: Option<DragState>,
}

pub const TITLE_BAR_H: f32 = 28.0; // 屏幕像素，标题栏高度（拖拽热区）
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;

fn pt_to_vec(p: Point<Pixels>) -> Vec2 {
    Vec2::new(f32::from(p.x), f32::from(p.y))
}

fn node_shadow(theme: Theme, focused: bool) -> Vec<BoxShadow> {
    let mut shadows = vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(theme.shadow_y)),
        blur_radius: px(theme.shadow_blur),
        spread_radius: px(theme.shadow_spread),
    }];

    if focused {
        shadows.push(BoxShadow {
            color: rgba(0x0a84ff30).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
        });
    }

    shadows
}

/// 滚轮 zoom，绕光标保持画布点不动。
fn on_scroll(
    app: &mut MultisoulApp,
    ev: &ScrollWheelEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let _span = tracing::trace_span!("canvas_zoom").entered();
    let cursor = pt_to_vec(ev.position);
    let vp = &mut app.runtime.document.viewport;
    let canvas_pt = vp.screen_to_canvas(cursor);

    let dy = f32::from(ev.delta.pixel_delta(px(1.0)).y); // 归一化滚动量
    let factor = (1.0 + dy * 0.0015).clamp(0.5, 1.5);
    vp.zoom = (vp.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    // 保持光标下画布点不动：pan = cursor - canvas_pt * zoom
    vp.pan = Vec2::new(
        cursor.x - canvas_pt.x * vp.zoom,
        cursor.y - canvas_pt.y * vp.zoom,
    );
    cx.notify();
}

fn on_mouse_down(
    app: &mut MultisoulApp,
    ev: &MouseDownEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);

    // 中键 或 空格+左键 → 开始 pan。
    let want_pan = ev.button == MouseButton::Middle
        || (ev.button == MouseButton::Left && app.interaction.space_held);
    if want_pan {
        app.interaction.panning = true;
        app.interaction.last_screen = Some(screen);
        return;
    }

    if ev.button != MouseButton::Left {
        return;
    }

    let canvas_pt = app.runtime.document.viewport.screen_to_canvas(screen);
    let hit = hit_test(app.runtime.document.nodes(), canvas_pt);

    // 双击终端 → 聚焦；否则单击选中/清空。
    let is_double = ev.click_count >= 2;
    let is_terminal = hit
        .and_then(|id| app.runtime.document.node(id))
        .map(|n| matches!(n.kind, CanvasNodeKind::Terminal(_)))
        .unwrap_or(false);

    let event = if is_double {
        InputEvent::DoubleClick { hit, is_terminal }
    } else {
        InputEvent::Click { hit }
    };

    match route(&app.runtime.focus.mode(), &event) {
        Route::SelectNode(id) => {
            app.runtime.selection.select_only(id);
            app.runtime.focus.exit_to_canvas();
            // 命中标题栏 → 开始拖拽。
            if let Some(node) = app.runtime.document.node(id) {
                let top_screen = app
                    .runtime
                    .document
                    .viewport
                    .canvas_to_screen(node.bounds.origin);
                if screen.y <= top_screen.y + TITLE_BAR_H * app.runtime.document.viewport.zoom {
                    app.interaction.drag = Some(DragState {
                        node: id,
                        last_screen: screen,
                    });
                }
            }
        }
        Route::ClearSelection => {
            app.runtime.selection.clear();
            app.runtime.focus.exit_to_canvas();
        }
        Route::FocusTerminal(id) => {
            app.runtime.focus.focus_terminal(id);
            app.runtime.selection.select_only(id);
        }
        _ => {}
    }
    cx.notify();
}

fn on_mouse_move(
    app: &mut MultisoulApp,
    ev: &MouseMoveEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    let screen = pt_to_vec(ev.position);

    if app.interaction.panning {
        if let Some(last) = app.interaction.last_screen {
            let delta = screen - last;
            let vp = &mut app.runtime.document.viewport;
            vp.pan = vp.pan + delta; // pan 直接按屏幕位移走
        }
        app.interaction.last_screen = Some(screen);
        cx.notify();
        return;
    }

    if let Some(drag) = app.interaction.drag {
        let zoom = app.runtime.document.viewport.zoom;
        let screen_delta = screen - drag.last_screen;
        // 屏幕位移 → 画布位移（除以 zoom）。
        let canvas_delta = Vec2::new(screen_delta.x / zoom, screen_delta.y / zoom);
        app.runtime.document.move_node(drag.node, canvas_delta);
        app.interaction.drag = Some(DragState {
            node: drag.node,
            last_screen: screen,
        });
        cx.notify();
    }
}

fn on_mouse_up(
    app: &mut MultisoulApp,
    _ev: &MouseUpEvent,
    _w: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    app.interaction.panning = false;
    app.interaction.drag = None;
    app.interaction.last_screen = None;
    cx.notify();
}

pub fn render_canvas(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let _span =
        tracing::trace_span!("render_canvas", nodes = app.runtime.document.nodes().len()).entered();

    let theme = app.theme;
    let vp = app.runtime.document.viewport;
    let win = window.viewport_size();
    let (win_w, win_h) = (f32::from(win.width), f32::from(win.height));
    let focus_mode = app.runtime.focus.mode();

    // 先收集需要渲染的节点数据（避免借用冲突）。
    let mut node_elems = Vec::new();
    let node_ids: Vec<NodeId> = app.runtime.document.nodes().iter().map(|n| n.id).collect();
    for id in node_ids {
        let (origin, size, selected, is_terminal, session_id) = {
            let n = app.runtime.document.node(id).unwrap();
            let sid = match n.kind {
                CanvasNodeKind::Terminal(r) => Some(r.session_id),
                _ => None,
            };
            (
                n.bounds.origin,
                n.bounds.size,
                n.selected,
                sid.is_some(),
                sid,
            )
        };
        let selected = selected || app.runtime.selection.contains(id);

        let top_left = vp.canvas_to_screen(origin);
        let w = size.x * vp.zoom;
        let h = size.y * vp.zoom;

        // viewport culling：完全在窗口外的节点跳过。
        if top_left.x + w < 0.0 || top_left.y + h < 0.0 || top_left.x > win_w || top_left.y > win_h
        {
            continue;
        }

        let focused = matches!(focus_mode, InputMode::TerminalFocused(fid) if fid == id);
        let stroke = if focused {
            theme.focused_stroke
        } else if selected {
            theme.selected_stroke
        } else {
            theme.node_stroke
        };

        let mut node_div = div()
            .absolute()
            .left(px(top_left.x))
            .top(px(top_left.y))
            .w(px(w))
            .h(px(h))
            .border_color(rgba(stroke))
            .shadow(node_shadow(theme, focused))
            .bg(rgba(theme.node_fill))
            .rounded(px(8.0))
            .overflow_hidden();

        node_div = if focused || selected {
            node_div.border_2()
        } else {
            node_div.border_1()
        };

        node_div = node_div.child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .h(px(1.0))
                .w_full()
                .bg(rgba(theme.edge_highlight)),
        );

        // 标题栏
        node_div = node_div.child(
            div()
                .h(px(TITLE_BAR_H * vp.zoom))
                .w_full()
                .bg(rgba(theme.title_bar))
                .text_color(rgba(theme.text))
                .px(px(8.0))
                .child(if is_terminal { "Terminal" } else { "Node" }),
        );

        // 内容
        if let Some(sid) = session_id {
            node_div = node_div.child(render_terminal_node(app, sid, vp.zoom, window, cx));
        }

        node_elems.push(node_div);
    }

    div()
        .id("canvas")
        .size_full()
        .relative()
        .bg(linear_gradient(
            145.0,
            linear_color_stop(rgba(theme.canvas_from), 0.0),
            linear_color_stop(rgba(theme.canvas_to), 1.0),
        ))
        .on_scroll_wheel(cx.listener(on_scroll))
        .on_mouse_down(MouseButton::Left, cx.listener(on_mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(on_mouse_down))
        .on_mouse_move(cx.listener(on_mouse_move))
        .on_mouse_up(MouseButton::Left, cx.listener(on_mouse_up))
        .on_mouse_up(MouseButton::Middle, cx.listener(on_mouse_up))
        .children(node_elems)
}
