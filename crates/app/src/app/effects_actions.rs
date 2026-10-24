//! 派发特效设置的加载、同步与保存。缓存放在 gpui Global 里，所有窗口共享；
//! 派发路径只读各窗口 `MotionStore` 里已解析好的选择，不碰 SQLite。

use crate::app::MultisoulApp;
use crate::settings::effects::page::EffectsPageData;
use crate::settings::effects::state::{
    EffectsSettingsState, EffectsTarget, PlaybackSpeed, Source, KEY_SPEED,
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
    snapshot: storage::DispatchEffectSnapshot,
    speed: PlaybackSpeed,
}

impl MultisoulApp {
    /// 宿主窗口启动时在后台读一次；会话窗口由宿主按键同步。
    pub(crate) fn load_effect_settings(&mut self, cx: &mut Context<Self>) {
        let catalog = self.desktop_workspace.store.clone();
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let work = cx.background_executor().spawn(async move {
            let snapshot = catalog.list_dispatch_effects().unwrap_or_else(|error| {
                tracing::warn!(%error, "dispatch effect settings load failed");
                Default::default()
            });
            LoadedEffects {
                snapshot,
                speed: PlaybackSpeed::from_key(
                    settings.get_string(KEY_SPEED).ok().flatten().as_deref(),
                ),
            }
        });
        cx.spawn(async move |this, cx| {
            let loaded = work.await;
            let _ = this.update(cx, |app, cx| {
                with_effects(cx, |state| {
                    state.load_snapshot(loaded.snapshot);
                    state.speed = loaded.speed;
                });
                app.sync_all_dispatch_effects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// 本窗口当前所处的 (工作空间, 项目)。
    fn effects_scope(&self) -> (Option<String>, Option<String>) {
        let context = self.wallpaper_settings.scoped.context.as_ref();
        (
            context.map(|context| context.workspace.id.clone()),
            context.and_then(|context| context.area.as_ref().map(|area| area.id.clone())),
        )
    }

    pub(crate) fn apply_dispatch_effects(
        &mut self,
        workspace: Option<&str>,
        area: Option<&str>,
        cx: &App,
    ) {
        let state = effects_snapshot(cx);
        self.motion
            .set_dispatch_style_selection(state.selection_for(workspace, area));
        self.motion.set_dispatch_speed(state.speed.multiplier());
    }

    pub(crate) fn sync_dispatch_effects(&mut self, cx: &App) {
        let (workspace, area) = self.effects_scope();
        self.apply_dispatch_effects(workspace.as_deref(), area.as_deref(), cx);
    }

    /// 宿主自己按当前上下文，各会话按自己的 (工作空间, 项目) 键同步。
    pub(crate) fn sync_all_dispatch_effects(&mut self, cx: &mut Context<Self>) {
        self.sync_dispatch_effects(cx);
        let sessions: Vec<_> = self
            .desktop_workspace
            .sessions
            .iter()
            .map(|((workspace, area), session)| (workspace.clone(), area.clone(), session.clone()))
            .collect();
        for (workspace, area, session) in sessions {
            session.update(cx, |session, cx| {
                session.apply_dispatch_effects(Some(&workspace), Some(&area), cx)
            });
        }
    }

    /// 设置页正在编辑的对象：由范围开关与当前上下文决定。
    pub(crate) fn effects_target(&self, edit_project: bool) -> Option<EffectsTarget> {
        let context = self.wallpaper_settings.scoped.context.as_ref()?;
        let workspace_id = context.workspace.id.clone();
        match (edit_project, context.area.as_ref()) {
            (true, Some(area)) => Some(EffectsTarget::Project {
                workspace_id,
                area_id: area.id.clone(),
            }),
            _ => Some(EffectsTarget::Workspace { workspace_id }),
        }
    }

    fn effects_scope_label(&self, target: &EffectsTarget) -> String {
        let Some(context) = self.wallpaper_settings.scoped.context.as_ref() else {
            return String::new();
        };
        match (target, context.area.as_ref()) {
            (EffectsTarget::Project { .. }, Some(area)) => {
                format!("{} / {}", context.workspace.name, area.display_name)
            }
            _ => context.workspace.name.clone(),
        }
    }

    pub(crate) fn set_effects_scope(&mut self, edit_project: bool, cx: &mut Context<Self>) {
        with_effects(cx, |state| {
            state.edit_project = edit_project;
            state.status = None;
        });
        cx.notify();
    }

    pub(crate) fn pick_effect_style(&mut self, style: &str, cx: &mut Context<Self>) {
        let Some(target) = self.target_for_edit(cx) else {
            return;
        };
        let next = effects_snapshot(cx).picked(&target, style);
        let label = ui::i18n::format(
            "effects.saved",
            &[
                ("scope", self.effects_scope_label(&target)),
                ("style", style_name(style).to_owned()),
            ],
        );
        self.save_effect(target, Some(next), Some(label), cx);
    }

    pub(crate) fn set_effect_source(
        &mut self,
        source: Source,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.target_for_edit(cx) else {
            return;
        };
        let next = effects_snapshot(cx).with_source(&target, source, value);
        self.save_effect(target, Some(next), None, cx);
    }

    pub(crate) fn set_effects_follow_workspace(&mut self, follows: bool, cx: &mut Context<Self>) {
        let Some(target @ EffectsTarget::Project { .. }) = self.target_for_edit(cx) else {
            return;
        };
        let next = (!follows).then(|| effects_snapshot(cx).unfollowed(&target));
        self.save_effect(target, next, None, cx);
    }

    fn target_for_edit(&self, cx: &App) -> Option<EffectsTarget> {
        self.effects_target(effects_snapshot(cx).edit_project)
    }

    fn save_effect(
        &mut self,
        target: EffectsTarget,
        next: Option<Pref>,
        label: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let previous = effects_snapshot(cx).saved(&target).cloned();
        if previous == next || effects_snapshot(cx).busy {
            return;
        }
        with_effects(cx, |state| {
            state.apply(&target, next.clone());
            state.busy = true;
            state.undo = Some((target.clone(), previous.clone()));
            state.status = label
                .clone()
                .or_else(|| Some(ui::i18n::text("effects.saving").to_owned()));
        });
        self.sync_all_dispatch_effects(cx);
        cx.notify();
        let catalog = self.desktop_workspace.store.clone();
        let write_target = target.clone();
        let write_value = next.clone();
        let work = cx.background_executor().spawn(async move {
            match (&write_target, &write_value) {
                (EffectsTarget::Workspace { workspace_id }, Some(pref)) => {
                    catalog.set_workspace_dispatch_effect(workspace_id, pref)
                }
                (EffectsTarget::Workspace { workspace_id }, None) => {
                    catalog.clear_workspace_dispatch_effect(workspace_id)
                }
                (
                    EffectsTarget::Project {
                        workspace_id,
                        area_id,
                    },
                    pref,
                ) => catalog.set_project_dispatch_effect(workspace_id, area_id, pref.as_ref()),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                with_effects(cx, |state| {
                    state.busy = false;
                    if let Err(error) = &result {
                        tracing::warn!(%error, ?target, "dispatch effect save failed");
                        state.apply(&target, previous);
                        state.undo = None;
                        state.status = Some(ui::i18n::text("effects.save_failed").to_owned());
                    } else if state.status.as_deref() == Some(ui::i18n::text("effects.saving")) {
                        // 没有专属提示的修改（来源覆盖、跟随开关）保存成功后收尾，不能一直停在“正在保存”。
                        state.status = Some(ui::i18n::text("norma.saved").to_owned());
                    }
                });
                app.sync_all_dispatch_effects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn undo_effect_change(&mut self, cx: &mut Context<Self>) {
        let Some((target, previous)) = with_effects(cx, |state| state.undo.take()) else {
            return;
        };
        self.save_effect(target, previous, None, cx);
        with_effects(cx, |state| state.undo = None);
    }

    pub(crate) fn set_effect_speed(&mut self, speed: PlaybackSpeed, cx: &mut Context<Self>) {
        with_effects(cx, |state| state.speed = speed);
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
        "ion" => [
            ui::i18n::text("effects.chip1_ion"),
            ui::i18n::text("effects.chip2_ion"),
        ],
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
        "random" => [
            ui::i18n::text("effects.chip1_random"),
            ui::i18n::text("effects.chip2_random"),
        ],
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
        let context = self.wallpaper_settings.scoped.context.as_ref();
        // 无障碍偏好只控制设置页的自动预览，派发始终播放选定的完整特效。
        let reduce_motion = self.motion.reduce_motion || crate::motion::system_reduce_motion();
        EffectsPageData {
            target: self.effects_target(state.edit_project),
            workspace_name: context.map(|context| context.workspace.name.clone()),
            project_name: context
                .and_then(|context| context.area.as_ref().map(|area| area.display_name.clone())),
            can_edit_project: context.is_some_and(|context| context.area.is_some()),
            reduce_motion,
            hover: self.effects_ui.hover,
            hover_since: self.effects_ui.hover_since,
            menu: self.effects_ui.menu,
            menu_cursor: self.effects_ui.menu_cursor,
            replay_started: self.effects_ui.replay_started,
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
        let saved = self
            .effects_target(state.edit_project)
            .and_then(|target| state.effective_for(&target).cloned())
            .and_then(|pref| match source {
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
