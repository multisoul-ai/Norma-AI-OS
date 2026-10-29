//! 想法拖拽的 GPUI 一侧：全窗口捕获层、预览条、目标高亮与松手投递。
//!
//! 设计注记（apple-design）：预览条 1:1 跟手并保持抓取偏移；悬停卡片即时高亮；
//! 松手才投递，落空或 Esc 取消则什么都不改变；鼠标移动只更新拖拽状态并按 16ms 合并重绘，
//! 命中用拖拽开始时缓存的矩形，不在移动路径里查库或读布局。
//! 减少动态效果下拖拽本身仍然跟手（直接操作不算装饰动效），只有流星改为无位移脉冲。

use super::idea_drag::{
    CardTarget, DragEvent, DragOutcome, DragTargets, DropTarget, IdeaDrag, Rect, RowSlot,
};
use super::ideas::IdeaUiTarget;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Stateful, Window,
};
use std::time::{Duration, Instant};
use ui::component::StyledExt;

const MOVE_NOTIFY_INTERVAL: Duration = Duration::from_millis(16);
/// 预览条抓取偏移：不复制整行，固定在条的左上附近。
const GRAB: (f32, f32) = (24.0, 16.0);
const ZONE_SIZE: (f32, f32) = (220.0, 42.0);
const ACCENT: u32 = 0x8fd0ffff;

pub(crate) struct IdeaDragSession {
    drag: IdeaDrag,
    targets: DragTargets,
    names: Vec<String>,
    title: String,
    last_notify: Option<Instant>,
}

impl MultisoulApp {
    pub(super) fn begin_idea_drag(
        &mut self,
        idea_id: &str,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.idea_drag.is_some() {
            return;
        }
        let Some(title) = self
            .desktop_workspace
            .ideas
            .iter()
            .find(|idea| idea.id == idea_id)
            .map(|idea| idea.body.lines().next().unwrap_or_default().to_owned())
        else {
            return;
        };
        // 没展开项目时也允许起拖：只有想法栏内的排序，没有卡片与新建区。
        let (targets, names) = self.idea_drag_targets(window, cx).unwrap_or_else(|| {
            (
                DragTargets::list_only(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    },
                    Vec::new(),
                ),
                Vec::new(),
            )
        });
        let start = (f32::from(event.position.x), f32::from(event.position.y));
        let row_origin = (start.0 - GRAB.0, start.1 - GRAB.1);
        self.desktop_workspace.idea_drag = Some(IdeaDragSession {
            drag: IdeaDrag::new(idea_id.to_owned(), start, row_origin),
            targets,
            names,
            title,
            last_notify: None,
        });
        self.desktop_workspace.ideas_menu = None;
        self.left_ideas_focus(cx).focus(window);
        cx.notify();
    }

    /// 窗口坐标下的卡片、新建区与可见画布；只取当前展开项目的智能体卡片。
    pub(super) fn idea_drag_targets(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<(DragTargets, Vec<String>)> {
        let session = self.expanded_desktop_project()?;
        let area_id = self.desktop_workspace.expanded_area_id.clone()?;
        let size = window.viewport_size();
        let frame = super::desktop_scene_rect(f32::from(size.width), f32::from(size.height));
        let layout = self.desktop_assistant_layout(window);
        let left = frame.origin.x.max(layout.left_space);
        let right = (frame.origin.x + frame.size.x).min(f32::from(size.width) - layout.right_space);
        let canvas = Rect {
            x: left,
            y: frame.origin.y,
            w: (right - left).max(1.0),
            h: frame.size.y,
        };
        let project = session.read(cx);
        let viewport = project.runtime.document.viewport;
        let mut cards = Vec::new();
        let mut names = Vec::new();
        for node in project.runtime.document.nodes() {
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                continue;
            };
            let rect = project.motion.current_node_rect(node.id, node.bounds);
            let origin = viewport.canvas_to_screen(rect.origin);
            cards.push(CardTarget {
                area_id: area_id.clone(),
                node_id: node.id,
                rect: Rect {
                    // 展开的项目画布整体右移了左抽屉占用的宽度（scene.rs 同样处理）。
                    x: origin.x + frame.origin.x + layout.left_space,
                    y: origin.y + frame.origin.y,
                    w: rect.size.x * viewport.zoom,
                    h: rect.size.y * viewport.zoom,
                },
            });
            names.push(
                spec.alias
                    .clone()
                    .unwrap_or_else(|| spec.profile.display_title().to_string()),
            );
        }
        let new_zone = Rect {
            x: canvas.x + (canvas.w - ZONE_SIZE.0) / 2.0,
            y: canvas.y + canvas.h - ZONE_SIZE.1 - 14.0,
            w: ZONE_SIZE.0,
            h: ZONE_SIZE.1,
        };
        Some((
            DragTargets {
                cards,
                new_zone,
                canvas,
                list: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                },
                rows: Vec::new(),
            },
            names,
        ))
    }

    /// 想法栏的行矩形来自上一帧的绘制结果；越过起拖阈值前每次移动都刷新，
    /// 这样起拖那一刻用的是菜单已收起后的真实布局。起拖后栏被捕获层遮住、不会再滚动，矩形保持不变。
    fn refresh_reorder_rows(&mut self) {
        let list = self.ideas_scroll.bounds();
        let rows: Vec<RowSlot> = self
            .desktop_workspace
            .ideas_aux
            .row_bounds
            .lock()
            .map(|rows| {
                rows.iter()
                    .map(|(id, bounds)| RowSlot {
                        id: id.clone(),
                        rect: rect_of(bounds),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let Some(session) = self.desktop_workspace.idea_drag.as_mut() else {
            return;
        };
        session.targets.list = rect_of(&list);
        session.targets.rows = rows;
    }

    fn idea_drag_move(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if self
            .desktop_workspace
            .idea_drag
            .as_ref()
            .is_some_and(|session| !session.drag.is_active())
        {
            self.refresh_reorder_rows();
        }
        let Some(session) = self.desktop_workspace.idea_drag.as_mut() else {
            return;
        };
        let event = session.drag.moved(x, y, &session.targets);
        let notify = match event {
            DragEvent::None => false,
            DragEvent::Started | DragEvent::HoverChanged => true,
            DragEvent::Moved => {
                let now = Instant::now();
                let due = session
                    .last_notify
                    .is_none_or(|last| now.duration_since(last) >= MOVE_NOTIFY_INTERVAL);
                if due {
                    session.last_notify = Some(now);
                }
                due
            }
        };
        if notify {
            cx.notify();
        }
    }

    fn idea_drag_finish(&mut self, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.desktop_workspace.idea_drag.take() else {
            return;
        };
        let id = session.drag.idea_id.clone();
        match session.drag.released(x, y, &session.targets) {
            DragOutcome::Click => {
                let screen = self.ideas_screen(cx);
                self.desktop_workspace.ideas_cursor =
                    screen.todo.iter().position(|row| row.id == id);
            }
            DragOutcome::Return => {}
            DragOutcome::Reorder { before } => self.reorder_idea(&id, before.as_deref(), cx),
            DragOutcome::Deliver { target, from, .. } => {
                let target = match target {
                    DropTarget::Existing { area_id, node_id } => {
                        IdeaUiTarget::Existing { area_id, node_id }
                    }
                    DropTarget::Reorder { .. } => {
                        cx.notify();
                        return;
                    }
                    DropTarget::NewAgent => {
                        let Some(area_id) = self.desktop_workspace.expanded_area_id.clone() else {
                            cx.notify();
                            return;
                        };
                        IdeaUiTarget::New {
                            area_id,
                            profile: self.desktop_workspace.ideas_aux.default_profile,
                        }
                    }
                };
                self.dispatch_idea(id, target, Some(from), window, cx);
            }
        }
        cx.notify();
    }

    /// Esc：取消拖拽。返回是否真的取消了一次拖拽。
    pub(super) fn cancel_idea_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.desktop_workspace.idea_drag.take() else {
            return false;
        };
        let _ = session.drag.cancel();
        cx.notify();
        true
    }

    pub(super) fn render_idea_drag_layer(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let session = self.desktop_workspace.idea_drag.as_ref()?;
        let active = session.drag.is_active();
        let hover = session.drag.hover().cloned();
        let (gx, gy) = session.drag.ghost_origin();
        let layer = div()
            .id("desktop-idea-drag-capture")
            .debug_selector(|| "desktop-idea-drag-capture".to_string())
            .absolute()
            .inset_0()
            .cursor_grabbing()
            .occlude()
            .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _window, cx| {
                cx.stop_propagation();
                let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                if event.pressed_button == Some(MouseButton::Left) {
                    app.idea_drag_move(x, y, cx);
                } else {
                    // 没有按下按键：松手发生在窗口外或失焦期间，按取消处理，绝不派发。
                    app.cancel_idea_drag(cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, event: &MouseUpEvent, window, cx| {
                    cx.stop_propagation();
                    let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                    app.idea_drag_finish(x, y, window, cx);
                }),
            );
        if !active {
            return Some(layer);
        }
        let cards = session.targets.cards.iter().zip(&session.names).map(|(card, name)| {
            let hot = matches!(&hover, Some(DropTarget::Existing { node_id, .. }) if *node_id == card.node_id);
            drop_outline(card.rect, ui::i18n::format("drawers.handoff", &[("name", (name).to_string())]), hot, "desktop-idea-drop-card")
        });
        let line = reorder_line(session, &hover);
        let zone_hot = hover == Some(DropTarget::NewAgent);
        // 没展开项目时没有新建区（矩形在无穷远），不画。
        let zone = session.targets.new_zone.x.is_finite().then(|| {
            drop_outline(
                session.targets.new_zone,
                ui::i18n::text("drawers.new_agent_plus").to_owned(),
                zone_hot,
                "desktop-idea-drop-new",
            )
        });
        let ghost = div()
            .id("desktop-idea-ghost")
            .debug_selector(|| "desktop-idea-ghost".to_string())
            .absolute()
            .left(px(gx))
            .top(px(gy))
            .max_w(px(280.0))
            .px(px(13.0))
            .py(px(9.0))
            .rounded(px(11.0))
            .border_1()
            .border_color(rgba(0x8fd0ffb8))
            .bg(rgba(0x123552ee))
            .text_size(px(12.5))
            .font_medium()
            .text_color(rgba(0xf1f9ffff))
            .overflow_hidden()
            .child(session.title.clone());
        Some(
            layer
                .children(cards)
                .children(zone)
                .children(line)
                .child(ghost),
        )
    }
}

fn drop_outline(rect: Rect, label: String, hot: bool, id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.w))
        .h(px(rect.h))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.0))
        .border_2()
        .border_color(rgba(if hot {
            0xbfe6ffff
        } else {
            (ACCENT & 0xffffff00) | 0x88
        }))
        .bg(rgba(if hot { 0x2a73a62e } else { 0x0b1b2d66 }))
        .child(
            div()
                .px(px(14.0))
                .py(px(7.0))
                .rounded_full()
                .bg(rgba(if hot { 0x1a4d73f2 } else { 0x0b1b2dd9 }))
                .text_size(px(12.0))
                .font_semibold()
                .text_color(rgba(if hot { 0xffffffff } else { 0xcfe8ffff }))
                .child(label),
        )
}

fn rect_of(bounds: &gpui::Bounds<gpui::Pixels>) -> Rect {
    Rect {
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        w: f32::from(bounds.size.width),
        h: f32::from(bounds.size.height),
    }
}

/// 排序的插入线：画在目标行之前（或最后一行之后）；落在原位置时不画，因为松手不会有任何变化。
fn reorder_line(session: &IdeaDragSession, hover: &Option<DropTarget>) -> Option<Stateful<Div>> {
    let Some(DropTarget::Reorder { before }) = hover else {
        return None;
    };
    let rows = &session.targets.rows;
    let own = rows.iter().position(|row| row.id == session.drag.idea_id)?;
    let slot = match before {
        Some(id) => rows.iter().position(|row| &row.id == id)?,
        None => rows.len(),
    };
    if slot == own || slot == own + 1 {
        return None;
    }
    let list = session.targets.list;
    let y = match before {
        Some(id) => rows.iter().find(|row| &row.id == id)?.rect.y - 2.0,
        None => {
            let last = rows.last()?;
            last.rect.y + last.rect.h + 1.0
        }
    };
    Some(
        div()
            .id("desktop-idea-reorder-line")
            .debug_selector(|| "desktop-idea-reorder-line".to_string())
            .absolute()
            .left(px(list.x + 12.0))
            .top(px(y))
            .w(px((list.w - 24.0).max(1.0)))
            .h(px(2.0))
            .rounded_full()
            .bg(rgba(0xbfe6ffff)),
    )
}
