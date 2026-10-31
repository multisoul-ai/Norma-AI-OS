//! 派发特效设置的加载、同步与保存。缓存放在 gpui Global 里，所有窗口共享；
//! 派发路径只读各窗口 `MotionStore` 里已解析好的选择，不碰 SQLite。

use crate::app::MultisoulApp;
use crate::motion::run_style::RunStyle;
use crate::settings::effects::page::EffectsPageData;
use crate::settings::effects::state::{
    EffectsSettingsState, PlaybackSpeed, Source, KEY_DISPATCH, KEY_RUN_STYLE, KEY_SPEED,
};
use gpui::{App, Context};
use storage::DispatchEffectPreference as Pref;

pub(crate) fn with_effects<R>(cx: &mut App, f: impl FnOnce(&mut EffectsSettingsState) -> R) -> R {
    if !cx.has_global::<EffectsSettingsState>() {
        cx.set_global(EffectsSettingsState::default());
    }
    f(cx.global_mut::<EffectsSettingsState>())
}

pub(crate) fn effects_snapshot(cx: &App) -> EffectsSettingsState {
    cx.try_global::<EffectsSettingsState>()
        .cloned()
        .unwrap_or_default()
}

struct LoadedEffects {
    preference: Option<Pref>,
    speed: PlaybackSpeed,
    run_style: Option<String>,
}

impl MultisoulApp {
    /// 宿主窗口启动时在后台读一次；会话窗口由宿主按键同步。
    pub(crate) fn load_effect_settings(&mut self, cx: &mut Context<Self>) {
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let work = cx.background_executor().spawn(async move {
            let preference = settings
                .get_string(KEY_DISPATCH)
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str::<Pref>(&json).ok())
                .filter(|pref| pref.validate().is_ok());
            LoadedEffects {
                preference,
                speed: PlaybackSpeed::from_key(
                    settings.get_string(KEY_SPEED).ok().flatten().as_deref(),
                ),
                run_style: settings.get_string(KEY_RUN_STYLE).ok().flatten(),
            }
        });
        cx.spawn(async move |this, cx| {
            let loaded = work.await;
            let _ = this.update(cx, |app, cx| {
                with_effects(cx, |state| {
                    if state.generation == 0 {
                        state.global = loaded.preference;
                    }
                    if state.speed_generation == 0 {
                        state.speed = loaded.speed;
                    }
                    // 用户在读取完成前已经点选过，就不要用旧值覆盖。
                    if state.run_generation == 0 {
                        state.run_style = RunStyle::from_id(loaded.run_style.as_deref());
                        state.run_saved = state.run_style;
                    }
                });
                app.notify_run_style_hosts(cx);
                app.sync_all_dispatch_effects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn sync_dispatch_effects(&mut self, cx: &App) {
        let state = effects_snapshot(cx);
        self.motion.set_dispatch_style_selection(state.selection());
        self.motion.set_dispatch_speed(state.speed.multiplier());
    }

    /// All open projects use the same global preference, including background sessions.
    pub(crate) fn sync_all_dispatch_effects(&mut self, cx: &mut Context<Self>) {
        self.sync_dispatch_effects(cx);
        let database = self.database_paths.app_db_path().to_owned();
        let sender = cx.entity();
        cx.defer(move |cx| {
            for view in crate::app::appearance::views(&database, sender, cx) {
                view.update(cx, |app, cx| {
                    app.sync_dispatch_effects(cx);
                    cx.notify();
                });
            }
        });
    }

    pub(crate) fn pick_effect_style(&mut self, style: &str, cx: &mut Context<Self>) {
        let next = effects_snapshot(cx).picked(style);
        self.save_effect(Some(next), cx);
    }

    pub(crate) fn set_effect_source(
        &mut self,
        source: Source,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let next = effects_snapshot(cx).with_source(source, value);
        self.save_effect(Some(next), cx);
    }

    fn save_effect(&mut self, next: Option<Pref>, cx: &mut Context<Self>) {
        // Serialize saves through the desktop host even when opened from a project view.
        let host = crate::app::appearance::coordinator(self, cx)
            .filter(|host| host.entity_id() != cx.entity_id());
        if let Some(host) = host {
            cx.defer(move |cx| host.update(cx, |app, cx| app.save_effect(next, cx)));
            return;
        }
        let previous = effects_snapshot(cx).global;
        if previous == next || effects_snapshot(cx).busy {
            return;
        }
        with_effects(cx, |state| {
            state.global = next.clone();
            state.generation += 1;
            state.busy = true;
            state.undo = Some(previous.clone());
            state.status = Some("effects.saving");
        });
        self.sync_all_dispatch_effects(cx);
        cx.notify();
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let work = cx.background_executor().spawn(async move {
            if let Some(pref) = &next {
                pref.validate()?;
            }
            let value = next.as_ref().map(serde_json::to_string).transpose()?;
            settings.apply_string_changes(&[(KEY_DISPATCH.into(), value)])
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                with_effects(cx, |state| {
                    state.busy = false;
                    if let Err(error) = &result {
                        tracing::warn!(%error, "global dispatch effect save failed");
                        state.global = previous;
                        state.undo = None;
                        state.status = Some("effects.save_failed");
                    } else {
                        state.status = Some("norma.saved");
                    }
                });
                app.sync_all_dispatch_effects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn undo_effect_change(&mut self, cx: &mut Context<Self>) {
        if effects_snapshot(cx).busy {
            return;
        }
        let host = crate::app::appearance::coordinator(self, cx)
            .filter(|host| host.entity_id() != cx.entity_id());
        if let Some(host) = host {
            cx.defer(move |cx| host.update(cx, |app, cx| app.undo_effect_change(cx)));
            return;
        }
        let Some(previous) = with_effects(cx, |state| state.undo.take()) else {
            return;
        };
        self.save_effect(previous, cx);
        with_effects(cx, |state| state.undo = None);
    }

    pub(crate) fn set_effect_speed(&mut self, speed: PlaybackSpeed, cx: &mut Context<Self>) {
        if let Some(host) = crate::app::appearance::coordinator(self, cx)
            .filter(|host| host.entity_id() != cx.entity_id())
        {
            cx.defer(move |cx| host.update(cx, |app, cx| app.set_effect_speed(speed, cx)));
            return;
        }
        with_effects(cx, |state| {
            state.speed = speed;
            state.speed_generation += 1;
        });
        self.persist_effect_option(KEY_SPEED, speed.key().into(), cx);
    }

    fn persist_effect_option(&mut self, key: &'static str, value: String, cx: &mut Context<Self>) {
        self.sync_all_dispatch_effects(cx);
        cx.notify();
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = settings.set_string(key, &value) {
                    tracing::warn!(%error, key, "effect option save failed");
                }
            })
            .detach();
    }
}

pub(crate) fn style_line(id: &str) -> &'static str {
    match id {
        "meteor" => ui::i18n::text("effects.line_meteor"),
        "ion" => ui::i18n::text("effects.line_ion"),
        "plane" => ui::i18n::text("effects.line_plane"),
        "ribbon" => ui::i18n::text("effects.line_ribbon"),
        "firefly" => ui::i18n::text("effects.line_firefly"),
        "fireworks" => ui::i18n::text("effects.line_fireworks"),
        "lightning" => ui::i18n::text("effects.line_lightning"),
        "vortex" => ui::i18n::text("effects.line_vortex"),
        "random" => ui::i18n::text("effects.line_random"),
        _ => ui::i18n::text("effects.line_meteor"),
    }
}

/// 说明里的两个情绪标签；第二个可能为空。
pub(crate) fn style_chips(id: &str) -> [&'static str; 2] {
    match id {
        "meteor" => [
            ui::i18n::text("effects.chip1_meteor"),
            ui::i18n::text("effects.chip2_meteor"),
        ],
        "ion" => [ui::i18n::text("effects.chip1_ion"), ""],
        "plane" => [
            ui::i18n::text("effects.chip1_plane"),
            ui::i18n::text("effects.chip2_plane"),
        ],
        "ribbon" => [
            ui::i18n::text("effects.chip1_ribbon"),
            ui::i18n::text("effects.chip2_ribbon"),
        ],
        "firefly" => [
            ui::i18n::text("effects.chip1_firefly"),
            ui::i18n::text("effects.chip2_firefly"),
        ],
        "fireworks" => [
            ui::i18n::text("effects.chip1_fireworks"),
            ui::i18n::text("effects.chip2_fireworks"),
        ],
        "lightning" => [
            ui::i18n::text("effects.chip1_lightning"),
            ui::i18n::text("effects.chip2_lightning"),
        ],
        "vortex" => [
            ui::i18n::text("effects.chip1_vortex"),
            ui::i18n::text("effects.chip2_vortex"),
        ],
        "random" => [ui::i18n::text("effects.chip1_random"), ""],
        _ => [
            ui::i18n::text("effects.chip1_meteor"),
            ui::i18n::text("effects.chip2_meteor"),
        ],
    }
}

pub(crate) fn style_name(id: &str) -> &'static str {
    match id {
        "meteor" => ui::i18n::text("effects.style_meteor"),
        "ion" => ui::i18n::text("effects.style_ion"),
        "plane" => ui::i18n::text("effects.style_plane"),
        "ribbon" => ui::i18n::text("effects.style_ribbon"),
        "firefly" => ui::i18n::text("effects.style_firefly"),
        "fireworks" => ui::i18n::text("effects.style_fireworks"),
        "lightning" => ui::i18n::text("effects.style_lightning"),
        "vortex" => ui::i18n::text("effects.style_vortex"),
        "random" => ui::i18n::text("effects.style_random"),
        _ => ui::i18n::text("effects.style_meteor"),
    }
}

impl MultisoulApp {
    pub(crate) fn effects_page_data(&self, cx: &App) -> EffectsPageData {
        let state = effects_snapshot(cx);
        // 无障碍偏好只控制设置页的自动预览，派发始终播放选定的完整特效。
        let reduce_motion = self.motion.reduce_motion || crate::motion::system_reduce_motion();
        EffectsPageData {
            reduce_motion,
            hover: self.effects_ui.hover,
            hover_since: self.effects_ui.hover_since,
            menu: self.effects_ui.menu,
            menu_cursor: self.effects_ui.menu_cursor,
            replay_started: self.effects_ui.replay_started,
            run: self.effects_ui.run,
            state,
        }
    }

    pub(crate) fn replay_effect_preview(&mut self, cx: &mut Context<Self>) {
        self.effects_ui.replay_started = Some(std::time::Instant::now());
        cx.notify();
    }

    pub(crate) fn set_effects_hover(
        &mut self,
        index: usize,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        if entered {
            self.effects_ui.hover = Some(index);
            self.effects_ui.hover_since = Some(std::time::Instant::now());
        } else if self.effects_ui.hover == Some(index) {
            self.effects_ui.hover = None;
            self.effects_ui.hover_since = None;
        }
        cx.notify();
    }

    fn saved_source_index(&self, source: Source, cx: &App) -> usize {
        let state = effects_snapshot(cx);
        let saved = state.global.and_then(|pref| match source {
            Source::Voice => pref.voice,
            Source::Idea => pref.idea,
            Source::Handoff => pref.handoff,
        });
        crate::settings::effects::sources::options(source)
            .iter()
            .position(|option| option.map(str::to_owned) == saved)
            .unwrap_or(0)
    }

    pub(crate) fn set_effect_menu(&mut self, source: Source, open: bool, cx: &mut Context<Self>) {
        if open {
            self.effects_ui.menu_cursor = self.saved_source_index(source, cx);
            self.effects_ui.menu = Some(source);
        } else if self.effects_ui.menu == Some(source) {
            self.effects_ui.menu = None;
        }
        cx.notify();
    }

    /// 返回 `true` 表示按键已被菜单处理。↑↓ 只移动浏览位置，Esc 与 Tab 关闭且不改设置。
    pub(crate) fn handle_effect_menu_key(
        &mut self,
        source: Source,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let count = crate::settings::effects::sources::options(source).len();
        if self.effects_ui.menu != Some(source) {
            if matches!(key, "enter" | "space" | "down") {
                self.set_effect_menu(source, true, cx);
                return true;
            }
            return false;
        }
        match key {
            "down" => self.effects_ui.menu_cursor = (self.effects_ui.menu_cursor + 1) % count,
            "up" => self.effects_ui.menu_cursor = (self.effects_ui.menu_cursor + count - 1) % count,
            "enter" | "space" => {
                let value = crate::settings::effects::sources::options(source)
                    [self.effects_ui.menu_cursor.min(count - 1)];
                self.choose_effect_source(source, value, cx);
                return true;
            }
            "escape" => self.effects_ui.menu = None,
            "tab" => {
                self.effects_ui.menu = None;
                cx.notify();
                return false;
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(crate) fn choose_effect_source(
        &mut self,
        source: Source,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.effects_ui.menu = None;
        self.set_effect_source(source, value, cx);
        cx.notify();
    }
}
