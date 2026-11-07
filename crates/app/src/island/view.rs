use super::{
    controller::{counts, IslandController},
    native::{self, HitPanel},
    style,
};
use gpui::{
    div, prelude::*, px, rgb, Context, Entity, FocusHandle, SharedString, Subscription, Window,
};
use runtime::island::transition::{IslandPresentation, IslandTransition};
use runtime::island::{geometry::IslandScreen, IslandEvent, IslandMode, IslandTab};
use std::{cell::RefCell, rc::Rc, time::Instant};
use ui::component::button::Button;
use ui::i18n::text;

pub(super) struct IslandView {
    pub display: u32,
    pub screen: IslandScreen,
    pub controller: Entity<IslandController>,
    focus: FocusHandle,
    panels: Rc<RefCell<Vec<HitPanel>>>,
    transition: IslandTransition,
    epoch: Instant,
    instant_revision: u64,
    body_height: f64,
    pub(super) staff_input: Option<Entity<ui::component::input::InputState>>,
    pub(super) caption_tick: Option<gpui::Task<()>>,
    pub(super) _staff_subscription: Option<Subscription>,
    _subscription: Subscription,
}

impl IslandView {
    pub fn new(
        display: u32,
        screen: IslandScreen,
        controller: Entity<IslandController>,
        panels: Rc<RefCell<Vec<HitPanel>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe_in(&controller, window, |view, _, window, cx| {
            view.sync(window, cx)
        });
        Self {
            display,
            screen,
            controller,
            panels,
            transition: IslandTransition::new(screen.layout(IslandMode::Compact)),
            epoch: Instant::now(),
            instant_revision: 0,
            body_height: 180.,
            staff_input: None,
            caption_tick: None,
            _staff_subscription: None,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    pub fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_layout(false, window, cx);
    }

    fn sync_layout(&mut self, measured: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.controller.read(cx).displays.mode_on(self.display) == IslandMode::Expanded
            && window.focused(cx).is_none()
        {
            // GPUI focus routing only: this does not activate the native App or panel.
            self.focus.focus(window);
        }
        let layout = self.screen.layout_for_content(
            self.controller.read(cx).displays.mode_on(self.display),
            self.body_height,
        );
        let instant_revision = self.controller.read(cx).instant_revision;
        let now = self.epoch.elapsed().as_millis() as u64;
        let reduce = crate::motion::system_reduce_motion()
            || crate::motion::util::reduce_motion_from_env_value(std::env::var_os(
                "MULTISOUL_REDUCE_MOTION",
            ));
        if measured && instant_revision == self.instant_revision {
            self.transition.retarget_content(layout, now, reduce);
        } else {
            self.transition.retarget(
                layout,
                now,
                reduce,
                instant_revision != self.instant_revision,
            );
        }
        self.instant_revision = instant_revision;
        self.presentation(window, cx);
        cx.notify();
    }

    fn presentation(&self, window: &mut Window, cx: &gpui::App) -> IslandPresentation {
        let presentation = self
            .transition
            .sample(self.epoch.elapsed().as_millis() as u64);
        let mut native_layout = presentation.surface;
        native_layout.frame = presentation.window;
        native::resize(window, native_layout, cx);
        if let Some(panel) = self
            .panels
            .borrow_mut()
            .iter_mut()
            .find(|p| p.display == self.display)
        {
            panel.layout = presentation.surface;
            let (x, y) = native::pointer();
            panel.panel.setIgnoresMouseEvents(
                self.controller.read(cx).displays.state.preferences().hidden
                    || !panel.layout.contains(x, y),
            );
        }
        presentation
    }

    pub fn focus_header(&self, window: &mut Window) {
        self.focus.focus(window);
    }

    pub fn event(&mut self, event: IslandEvent, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, cx| {
            controller.event(self.display, event, cx)
        });
    }
}

impl Render for IslandView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.mount_questions(window, cx);
        let presentation = self.presentation(window, cx);
        if presentation.animating {
            window.request_animation_frame();
        }
        let controller = self.controller.read(cx);
        let mode = controller.displays.mode_on(self.display);
        let confirmation =
            mode == IslandMode::Activity && controller.displays.state.activity_id().is_none();
        let tab = controller.displays.state.tab();
        let workspaces = controller.visible_workspaces(cx);
        let (running, pending) = counts(&workspaces);
        let layout = presentation.surface;
        let message = controller.message.clone();
        let completion_label = controller.completion_label.clone().unwrap_or_default();
        let wrap = div().size_full().flex().justify_center();
        let mut root = div()
            .id("norma-island")
            .track_focus(&self.focus)
            .w(px(layout.frame.width as f32))
            .h(px(layout.frame.height as f32))
            .flex_none()
            .overflow_hidden()
            .opacity(presentation.opacity)
            .flex()
            .flex_col()
            .bg(rgb(0x000000))
            .text_color(rgb(0xf4f6f8))
            .text_size(px(12.))
            .rounded_b(px(layout.radius as f32))
            .when(layout.notch_width == 0., |div| {
                div.rounded_t(px(layout.radius as f32))
            })
            .on_hover(cx.listener(|view, inside, _, cx| {
                view.event(
                    if *inside {
                        IslandEvent::PointerEntered
                    } else {
                        IslandEvent::PointerLeft
                    },
                    cx,
                )
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    view.event(IslandEvent::Escape, cx);
                    cx.stop_propagation();
                }
            }));
        root = root.child(self.header(layout, &workspaces, running, pending, cx));
        if mode == IslandMode::Compact {
            return wrap.child(root);
        }
        if mode == IslandMode::Expanded {
            root = root.child(self.tabs(running, pending, presentation.content_inset(), cx));
        }
        let mut body = div()
            .w(px(self.screen.layout(mode).frame.width as f32))
            .flex_none()
            .px(px(16.))
            .pt(px(match mode {
                IslandMode::Activity => 4.,
                IslandMode::Alert => 6.,
                _ => 12.,
            }))
            .flex()
            .flex_col()
            .gap(px(if mode == IslandMode::Expanded {
                14.
            } else {
                10.
            }));
        if let Some(message) = message {
            body = body.child(
                div()
                    .id("island-feedback")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(confirmation, |row| {
                        row.child(
                            ui::component::Icon::new(ui::component::IconName::Check)
                                .size(px(14.))
                                .text_color(rgb(style::OK)),
                        )
                    })
                    .child(message.to_string())
                    .on_click(cx.listener(|view, _, _, cx| view.event(IslandEvent::Expand, cx))),
            );
        }
        if mode == IslandMode::Activity {
            if !confirmation {
                body = body.child(
                    div()
                        .id("island-completed")
                        .cursor_pointer()
                        .child(format!(
                            "{} · {}",
                            completion_label,
                            text("island.completed")
                        ))
                        .on_click(
                            cx.listener(|view, _, _, cx| view.event(IslandEvent::Expand, cx)),
                        ),
                );
            }
        } else if tab == IslandTab::Staff && mode == IslandMode::Expanded {
            body = body.child(self.staff_tab(window, cx));
        } else {
            if mode == IslandMode::Expanded
                && ((tab == IslandTab::Running
                    && workspaces.iter().all(|w| {
                        w.projects.iter().all(|p| {
                            p.overview.running.is_empty()
                                && p.overview.idle.is_empty()
                                && p.overview.unloaded_projects.is_empty()
                                && !p.overview.attention.iter().any(|a| {
                                    matches!(
                                        a.kind,
                                        norma::mobile::RunOverviewAttentionKind::Failed
                                            | norma::mobile::RunOverviewAttentionKind::Takeover
                                    )
                                })
                        })
                    }))
                    || (tab == IslandTab::Pending && pending == 0))
            {
                body = body.child(style::empty(
                    if tab == IslandTab::Running {
                        "island.no_running"
                    } else {
                        "island.no_pending"
                    },
                    if tab == IslandTab::Running {
                        "island.no_running_hint"
                    } else {
                        "island.no_pending_hint"
                    },
                ));
            }
            for workspace in &workspaces {
                let mut rows = Vec::new();
                for project in &workspace.projects {
                    if tab == IslandTab::Running && mode == IslandMode::Expanded {
                        rows.extend(self.render_running(workspace, project, cx));
                    } else {
                        rows.extend(self.render_pending(
                            workspace,
                            project,
                            mode == IslandMode::Alert,
                            cx,
                        ));
                    }
                }
                if rows.is_empty() {
                    continue;
                }
                let mut group = div()
                    .flex()
                    .flex_col()
                    .gap(px(if tab == IslandTab::Running { 4. } else { 14. }));
                if tab == IslandTab::Running && mode == IslandMode::Expanded {
                    group = group.child(
                        div()
                            .px(px(4.))
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(workspace.name.clone())
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(rgb(style::FAINT))
                                    .child(super::running::workspace_summary(workspace)),
                            ),
                    );
                }
                body = body.child(group.children(rows));
            }
        }
        let entity = cx.entity().downgrade();
        let viewport = div()
            .flex_1()
            .min_h(px(0.))
            // The bottom inset keeps opaque scrolling children inside the curved silhouette.
            .mb(px(presentation.content_inset() as f32))
            .child(body)
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(bounds) = bounds.first() else { return };
                let height = f64::from(f32::from(bounds.size.height))
                    + 16.
                    + if mode == IslandMode::Expanded {
                        40.
                    } else {
                        0.
                    };
                let entity = entity.clone();
                window.defer(cx, move |window, cx| {
                    let _ = entity.update(cx, |view, cx| {
                        let state = view.controller.read(cx);
                        if state.displays.mode_on(view.display) == mode
                            && state.displays.state.tab() == tab
                            && (view.body_height - height).abs() > 1.
                        {
                            view.body_height = height;
                            view.sync_layout(true, window, cx);
                        }
                    });
                });
            })
            .id("island-body")
            .overflow_y_scroll();
        wrap.child(root.child(viewport))
    }
}

pub(super) fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &gpui::App,
) -> Button {
    style::action(
        id,
        label,
        if selected {
            style::Tone::Primary
        } else {
            style::Tone::Neutral
        },
        cx,
    )
}
