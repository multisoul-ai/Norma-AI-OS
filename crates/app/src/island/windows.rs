use super::{
    controller::IslandController,
    native::{self, HitPanel, NativeEvent, Watch},
    view::IslandView,
};
use gpui::{
    px, size, AnyWindowHandle, App, AppContext, Bounds, Context, Entity, Global,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};
use runtime::island::{geometry::IslandScreen, IslandEvent, IslandMode};
use std::collections::BTreeMap;
use ui::component::Root;

struct IslandWindows {
    _manager: Entity<WindowManager>,
}
#[derive(Default)]
struct IslandWindowIds(Vec<AnyWindowHandle>);
impl Global for IslandWindowIds {}
impl Global for IslandWindows {}

struct WindowManager {
    controller: Entity<IslandController>,
    windows: BTreeMap<u32, (AnyWindowHandle, Entity<IslandView>)>,
    watch: Watch,
    pointer_display: Option<u32>,
    hidden: bool,
    _settings_subscription: Option<gpui::Subscription>,
}

pub(crate) fn install(cx: &mut App) {
    if cx.has_global::<IslandWindows>() {
        return;
    }
    cx.set_global(IslandWindowIds::default());
    let model = super::model(cx);
    let controller = cx.new(|cx| IslandController::new(model, cx));
    let (tx, rx) = async_channel::unbounded();
    let manager = cx.new(|cx| {
        let subscription = super::preferences::state(cx).map(|settings| {
            cx.observe(&settings, |this: &mut WindowManager, settings, cx| {
                let hidden = settings.read(cx).preferences.hidden;
                if this.hidden != hidden {
                    this.hidden = hidden;
                    this.refresh(cx);
                    for panel in this.watch.panels.borrow().iter() {
                        native::set_visible(panel.panel.clone(), !hidden, cx);
                    }
                }
            })
        });
        let mut manager = WindowManager {
            controller,
            windows: BTreeMap::new(),
            watch: Watch::new(tx),
            pointer_display: None,
            hidden: super::preferences::current(cx).hidden,
            _settings_subscription: subscription,
        };
        manager.refresh(cx);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if this
                    .update(cx, |this, cx| match event {
                        NativeEvent::ScreensChanged => this.refresh(cx),
                        NativeEvent::Pointer => this.pointer(cx),
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        manager
    });
    cx.set_global(IslandWindows { _manager: manager });
}

pub(crate) fn has_workspace_windows(cx: &App) -> bool {
    let island = cx.try_global::<IslandWindowIds>();
    cx.windows()
        .iter()
        .any(|window| island.is_none_or(|island| !island.0.contains(window)))
}

impl WindowManager {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let screens = native::screens();
        // The debug driver's private display isolates all test surfaces from the user's desktop.
        #[cfg(all(debug_assertions, not(feature = "production-data")))]
        let screens: Vec<_> = screens
            .into_iter()
            .filter(|(id, _)| {
                crate::e2e_control::target_display(cx)
                    .is_none_or(|display| u32::from(display) == *id)
            })
            .collect();
        let ids: Vec<_> = screens.iter().map(|(id, _)| *id).collect();
        let removed: Vec<_> = self
            .windows
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in removed {
            if let Some((window, _)) = self.windows.remove(&id) {
                // Close observers run synchronously while this manager is leased.
                // They read the independent handle snapshot, never this Entity.
                let _ = window.update(cx, |_, window, _| window.remove_window());
                cx.global_mut::<IslandWindowIds>()
                    .0
                    .retain(|id| *id != window);
            }
        }
        self.watch
            .panels
            .borrow_mut()
            .retain(|p| ids.contains(&p.display));
        self.controller.update(cx, |controller, cx| {
            controller.retain_displays(&ids, cx);
        });
        for (id, screen) in screens {
            if let Some((window, view)) = self.windows.get(&id) {
                let _ = window.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.screen = screen;
                        view.sync(window, cx);
                    });
                });
            } else if !self.hidden {
                if let Err(error) = self.open(id, screen, cx) {
                    tracing::warn!(%error, display = id, "could not open island window");
                }
            }
        }
        self.pointer(cx);
    }

    fn open(
        &mut self,
        id: u32,
        screen: IslandScreen,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let controller = self.controller.clone();
        let panels = self.watch.panels.clone();
        let layout = screen.layout(IslandMode::Compact);
        let display_id = cx
            .displays()
            .into_iter()
            .find(|d| u32::from(d.id()) == id)
            .map(|d| d.id());
        let mut view = None;
        let bounds = Bounds::centered(
            display_id,
            size(
                px(layout.frame.width as f32),
                px(layout.frame.height as f32),
            ),
            cx,
        );
        let handle = cx.open_window(
            WindowOptions {
                kind: WindowKind::PopUp,
                focus: false,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                display_id,
                window_background: WindowBackgroundAppearance::Transparent,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                native::configure(window, cx);
                let island = cx
                    .new(|cx| IslandView::new(id, screen, controller, panels.clone(), window, cx));
                if let Some(panel) = native::panel(window) {
                    panels.borrow_mut().push(HitPanel {
                        display: id,
                        panel,
                        layout,
                    });
                }
                island.update(cx, |view, cx| view.sync(window, cx));
                view = Some(island.clone());
                cx.new(|cx| Root::new(island, window, cx))
            },
        )?;
        if let Some(view) = view {
            self.windows.insert(id, (handle.into(), view));
            cx.global_mut::<IslandWindowIds>().0.push(handle.into());
        }
        Ok(())
    }

    fn pointer(&mut self, cx: &mut Context<Self>) {
        let next = (!self.hidden)
            .then(|| self.watch.pointer_display())
            .flatten();
        if next == self.pointer_display {
            return;
        }
        let previous = self.pointer_display;
        self.pointer_display = next;
        self.controller.update(cx, |controller, cx| {
            if let Some(id) = previous {
                controller.event(id, IslandEvent::PointerLeft, cx);
            }
            if let Some(id) = next {
                controller.event(id, IslandEvent::PointerEntered, cx);
            }
        });
    }
}
