use super::{
    controller::{counts, IslandController},
    native::{self, HitPanel},
    style,
};
use gpui::{
    div, prelude::*, px, rgb, Context, Entity, FocusHandle, SharedString, Subscription, Window,
};
use runtime::island::transition::{IslandTransition, Morph};
use runtime::island::{geometry::IslandScreen, IslandEvent, IslandMode};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use ui::component::button::Button;

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
    /// While the compositor shrinks the shape, the previous (larger) content stays rendered.
    shown_mode: Option<IslandMode>,
    last_mode: IslandMode,
    /// Pending end-of-morph work (release the transparent window area, show the final content).
    settle: Option<gpui::Task<()>>,
    /// The next morph waits for one unconstrained content measurement.
    measure_before_morph: bool,
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
            shown_mode: None,
            last_mode: IslandMode::Compact,
            settle: None,
            measure_before_morph: false,
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
        let mode_now = self.controller.read(cx).displays.mode_on(self.display);
        if mode_now != IslandMode::Compact && window.focused(cx).is_none() {
            // GPUI focus routing only: this does not activate the native App or panel.
            // Alert and activity must receive Escape the same way expanded does.
            self.focus.focus(window);
        }
        let mode = self.controller.read(cx).displays.mode_on(self.display);
        // One paint at the content's own height, then a single morph. Measuring
        // again after the window has grown restarts the animation and stutters.
        if !measured
            && self.settle.is_none()
            && mode != self.last_mode
            && mode != IslandMode::Compact
        {
            self.measure_before_morph = true;
            cx.notify();
            return;
        }
        self.measure_before_morph = false;
        let workspaces = self.controller.read(cx).visible_workspaces(cx);
        let faces = super::faces::collect(&workspaces);
        let roster = runtime::island::roster::compact_roster(&super::faces::slots(&faces));
        let compact =
            runtime::island::geometry::compact_width(roster.running_total(), roster.pending_total);
        let layout = self
            .screen
            .layout_for_content_width(mode, self.body_height, compact);
        let instant_revision = self.controller.read(cx).instant_revision;
        let now = self.epoch.elapsed().as_millis() as u64;
        let reduce = !animations_on();
        let morph = if measured && instant_revision == self.instant_revision {
            // Already in this mode: a tab or a late measurement only changes
            // height. Replaying the expand morph moves the tab buttons.
            self.transition
                .retarget(layout, now, reduce, mode == self.last_mode)
        } else {
            self.transition.retarget(
                layout,
                now,
                reduce,
                instant_revision != self.instant_revision,
            )
        };
        self.instant_revision = instant_revision;
        self.hold_content(morph, window, cx);
        self.push_shape(morph, now, window, cx);
        self.last_mode = mode;
        self.update_hit_region(cx);
        cx.notify();
    }

    /// A shrinking shape keeps the larger content until it has finished; growing, immediate and
    /// unchanged targets never hold. `settle` runs once the compositor is done.
    fn hold_content(&mut self, morph: Option<Morph>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(morph) = morph else {
            // Same target while a morph is running keeps the hold; anything else clears it.
            if self.settle.is_some() && self.transition.sample(self.now_ms()).animating {
                return;
            }
            self.settle = None;
            self.shown_mode = None;
            return;
        };
        let shrinking = !morph.fade_only
            && (morph.to.frame.height < morph.from.frame.height
                || morph.to.frame.width < morph.from.frame.width);
        self.shown_mode = shrinking.then(|| self.shown_mode.unwrap_or(self.last_mode));
        let wait = Duration::from_millis(morph.duration_ms + 24);
        self.settle = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(wait).await;
            let _ = view.update_in(cx, |view, window, cx| view.settle(window, cx));
        }));
    }

    fn settle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settle = None;
        self.shown_mode = None;
        self.sync_layout(false, window, cx);
    }

    /// Hands the compositor the window frame and shape. During a morph the window keeps room for
    /// both ends; the animation itself is a one-shot layer animation, not a per-frame callback.
    fn push_shape(
        &self,
        morph: Option<Morph>,
        now: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if morph.is_none() && self.settle.is_some() {
            return;
        }
        let held = self.settle.is_some();
        native::apply_shape(
            window,
            native::Shape {
                window: if held {
                    self.transition.envelope()
                } else {
                    self.transition.sample(now).window
                },
                // sample() is the in-flight shape. A morph must end on its target,
                // which at t=0 is not the sample.
                surface: morph
                    .map(|morph| morph.to)
                    .unwrap_or_else(|| self.transition.sample(now).surface),
                morph,
            },
            cx,
        );
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn update_hit_region(&self, cx: &gpui::App) {
        let layout = self.transition.content(self.settle.is_some());
        if let Some(panel) = self
            .panels
            .borrow_mut()
            .iter_mut()
            .find(|p| p.display == self.display)
        {
            panel.layout = layout;
            let (x, y) = native::pointer();
            panel.panel.setIgnoresMouseEvents(
                self.controller.read(cx).displays.state.preferences().hidden
                    || !panel.layout.contains(x, y),
            );
        }
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

fn animations_on() -> bool {
    !(crate::motion::system_reduce_motion()
        || crate::motion::util::reduce_motion_from_env_value(std::env::var_os(
            "MULTISOUL_REDUCE_MOTION",
        )))
}

impl Render for IslandView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let animate = animations_on();
        super::style::set_animations(animate);
        self.mount_questions(window, cx);
        let held = self.settle.is_some();
        let measuring = self.measure_before_morph;
        let controller = self.controller.read(cx);
        let mode = self
            .shown_mode
            .unwrap_or_else(|| controller.displays.mode_on(self.display));
        let confirmation =
            mode == IslandMode::Activity && controller.displays.state.activity_id().is_none();
        let tab = controller.displays.state.tab();
        let workspaces = controller.visible_workspaces(cx);
        let (running, pending) = counts(&workspaces);
        let layout = if measuring {
            self.screen.layout_for_content(mode, self.body_height)
        } else {
            self.transition.content(held)
        };
        let inset = if held {
            layout.radius.ceil().max(16.)
        } else {
            16.
        };
        let message = controller.message.clone();
        let completion_label = controller.completion_label.clone().unwrap_or_default();
        let faces = super::faces::collect(&workspaces);
        let queue = super::faces::queue_items(&workspaces);
        let activity_deadline = (mode == IslandMode::Activity
            && controller.displays.state.activity_id().is_some())
        .then(|| controller.displays.state.deadline_ms())
        .flatten();
        let mut wrap = div().size_full().flex().justify_center();
        if measuring {
            // Don't stretch the column to the still-compact window, or the
            // measurement is the window instead of the content.
            wrap = wrap.items_start();
        }
        let mut root = div()
            .id("norma-island")
            .track_focus(&self.focus)
            .w(px(layout.frame.width as f32))
            .when(!measuring, |div| div.h(px(layout.frame.height as f32)))
            .flex_none()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(0x000000))
            .text_color(rgb(0xf4f6f8))
            .text_size(px(12.))
            .relative()
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
        root = root.child(self.header(layout, &workspaces, &faces, &queue, animate, cx));
        if let Some(deadline) = activity_deadline {
            root = root.child(super::activity_view::countdown(
                self.now_ms(),
                deadline,
                animate,
            ));
        }
        if mode == IslandMode::Compact {
            return wrap.child(root);
        }
        if mode == IslandMode::Expanded {
            root = root.child(self.tabs(running, pending, inset, cx));
        }
        let body = self.island_body(
            super::body::BodyInput {
                mode,
                tab,
                workspaces: &workspaces,
                faces: &faces,
                queue: &queue,
                pending,
                message,
                confirmation,
                completion_label: &completion_label,
                animate,
            },
            window,
            cx,
        );
        let entity = cx.entity().downgrade();
        let mut viewport = div();
        if measuring {
            viewport = viewport.flex_none();
        } else {
            viewport = viewport.flex_1().min_h(px(0.));
        }
        let viewport = viewport
            // The bottom inset keeps opaque scrolling children inside the curved silhouette.
            .mb(px(inset as f32))
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
                        // A measurement that arrives while the mask is moving used to
                        // retarget and restart the morph. That restart is the stutter.
                        if view.transition.blocks_content_measurement(view.now_ms()) {
                            return;
                        }
                        let state = view.controller.read(cx);
                        if state.displays.mode_on(view.display) != mode
                            || state.displays.state.tab() != tab
                        {
                            return;
                        }
                        let changed = (view.body_height - height).abs() > 1.;
                        if !view.measure_before_morph && !changed {
                            return;
                        }
                        view.body_height = height;
                        view.sync_layout(true, window, cx);
                    });
                });
            })
            .id("island-body")
            .when(!measuring, |div| div.overflow_y_scroll());
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
