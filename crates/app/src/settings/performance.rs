use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use storage::SettingsStore;
use ui::component::switch::Switch;
use ui::component::{Sizable as _, Size};
use ui::Theme;

pub(super) const IDLE_ANIMATION_THROTTLING_KEY: &str = "performance.idle_animation_throttling.v1";

pub(crate) struct PerformanceSettingsState {
    storage: Option<SettingsStore>,
    idle_animation_throttling_enabled: bool,
    pending_idle_animation_throttling: Option<bool>,
    persistence_running: bool,
}

impl PerformanceSettingsState {
    pub(crate) fn load_from_store(store: SettingsStore) -> Self {
        Self::load_from(Some(store))
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn in_memory_default() -> Self {
        Self::load_from(None)
    }

    fn load_from(storage: Option<SettingsStore>) -> Self {
        let enabled = storage
            .as_ref()
            .and_then(
                |store| match store.get_string(IDLE_ANIMATION_THROTTLING_KEY) {
                    Ok(saved) => saved,
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to load performance setting");
                        None
                    }
                },
            )
            .and_then(|saved| match saved.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => {
                    tracing::warn!(saved, "ignored unknown idle animation throttling value");
                    None
                }
            })
            .unwrap_or(false);
        Self {
            storage,
            idle_animation_throttling_enabled: enabled,
            pending_idle_animation_throttling: None,
            persistence_running: false,
        }
    }

    pub(crate) fn idle_animation_throttling_enabled(&self) -> bool {
        self.idle_animation_throttling_enabled
    }

    fn queue_idle_animation_throttling(&mut self, enabled: bool) {
        self.idle_animation_throttling_enabled = enabled;
        self.pending_idle_animation_throttling = Some(enabled);
    }

    fn take_pending_write(&mut self) -> Option<(SettingsStore, bool)> {
        if self.persistence_running {
            return None;
        }
        let enabled = self.pending_idle_animation_throttling.take()?;
        let store = self.storage.clone()?;
        self.persistence_running = true;
        Some((store, enabled))
    }

    fn finish_write(&mut self) {
        self.persistence_running = false;
    }
}

impl MultisoulApp {
    pub(crate) fn set_idle_animation_throttling(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.performance_settings
            .queue_idle_animation_throttling(enabled);
        self.wake_animation_frame_loop();
        self.persist_performance_settings_in_background(cx);
        cx.notify();
    }

    fn persist_performance_settings_in_background(&mut self, cx: &mut Context<Self>) {
        let Some((store, enabled)) = self.performance_settings.take_pending_write() else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            store
                .set_string(
                    IDLE_ANIMATION_THROTTLING_KEY,
                    if enabled { "true" } else { "false" },
                )
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.performance_settings.finish_write();
                if let Err(error) = result {
                    tracing::warn!(error, "failed to persist performance setting");
                }
                app.persist_performance_settings_in_background(cx);
            });
        })
        .detach();
    }
}

pub(super) fn performance_settings(
    theme: Theme,
    state: &PerformanceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let enabled = state.idle_animation_throttling_enabled();
    let app = cx.weak_entity();
    let status = if enabled {
        ui::i18n::text("settings.energy_saver")
    } else {
        ui::i18n::text("settings.smoothest_response")
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .settings_text(TextRole::Headline, theme)
                        .child(ui::i18n::text("settings.performance_energy")),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text("settings.choose_between_immediate_animation_response_and_lower_idle_work")),
                ),
        )
        .child(
            div()
                .w_full()
                .min_h(px(92.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(20.0))
                .px(px(18.0))
                .py(px(16.0))
                .rounded(px(16.0))
                .border_1()
                .border_color(rgba(theme.node_stroke))
                .bg(rgba(theme.panel_high))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(9.0))
                                .child(
                                    div()
                                        .settings_text(TextRole::RowTitle, theme)
                                        .child(ui::i18n::text("settings.reduce_animation_updates_while_idle")),
                                )
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(9.0))
                                        .bg(rgba(theme.panel))
                                        .settings_text(TextRole::Badge, theme)
                                        .child(status),
                                ),
                        )
                        .child(
                            div()
                                .max_w(px(570.0))
                                .settings_text(TextRole::Helper, theme)
                                .child(
                                    ui::i18n::text("settings.checks_for_new_canvas_animations_every_250_ms_while_idle_leave"),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("settings-idle-animation-throttling")
                        .debug_selector(|| {
                            "settings-idle-animation-throttling".to_string()
                        })
                        .w(px(36.0))
                        .h(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Switch::new("settings-idle-animation-throttling-switch")
                                .checked(enabled)
                                .with_size(Size::Medium)
                                .tooltip(ui::i18n::text("settings.reduce_animation_updates_while_the_workspace_is_idle"))
                                .on_click(move |checked, _window, cx| {
                                    let _ = app.update(cx, |app, cx| {
                                        app.set_idle_animation_throttling(*checked, cx);
                                    });
                                }),
                        ),
                ),
        )
}

#[cfg(test)]
#[path = "../../tests/unit/performance_settings_tests.rs"]
mod tests;
