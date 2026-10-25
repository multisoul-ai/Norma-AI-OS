//! 卡片运行特效风格的读取、保存与撤销。全局偏好，存在 `effects.run_style`；
//! 渲染只读 gpui Global 缓存，不碰 SQLite。

use crate::app::effects_actions::with_effects;
use crate::app::MultisoulApp;
use crate::motion::run_style::{RunState, RunStyle};
use crate::settings::effects::state::{EffectsSettingsState, KEY_RUN_STYLE};
use gpui::{App, Context};

/// 当前生效的运行特效风格；设置尚未加载时为经典。
pub(crate) fn run_style(cx: &App) -> RunStyle {
    cx.try_global::<EffectsSettingsState>()
        .map(|state| state.run_style)
        .unwrap_or_default()
}

pub(crate) fn run_style_name(style: RunStyle) -> &'static str {
    match style {
        RunStyle::Classic => ui::i18n::text("effects.run_classic"),
        RunStyle::Underline => ui::i18n::text("effects.run_underline"),
        RunStyle::Aura => ui::i18n::text("effects.run_aura"),
        RunStyle::Ferrofluid => ui::i18n::text("effects.run_ferrofluid"),
    }
}

pub(crate) fn run_style_line(style: RunStyle) -> &'static str {
    match style {
        RunStyle::Classic => ui::i18n::text("effects.run_line_classic"),
        RunStyle::Underline => ui::i18n::text("effects.run_line_underline"),
        RunStyle::Aura => ui::i18n::text("effects.run_line_aura"),
        RunStyle::Ferrofluid => ui::i18n::text("effects.run_line_ferrofluid"),
    }
}

impl MultisoulApp {
    /// 风格是全局的：宿主与所有项目会话的画布都要重绘。
    pub(crate) fn notify_run_style_hosts(&mut self, cx: &mut Context<Self>) {
        let sessions: Vec<_> = self.desktop_workspace.sessions.values().cloned().collect();
        for session in sessions {
            session.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    pub(crate) fn pick_run_style(&mut self, style: RunStyle, cx: &mut Context<Self>) {
        let previous = run_style(cx);
        if previous == style {
            return;
        }
        let label = ui::i18n::format(
            "effects.run_saved",
            &[("style", run_style_name(style).to_owned())],
        );
        self.save_run_style(style, Some(previous), label, cx);
    }

    pub(crate) fn undo_run_style(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = with_effects(cx, |state| state.run_undo.take()) else {
            return;
        };
        let label = ui::i18n::format(
            "effects.run_saved",
            &[("style", run_style_name(previous).to_owned())],
        );
        self.save_run_style(previous, None, label, cx);
    }

    /// 预览固定某个状态；`None` 恢复自动轮播。都从头计时。
    pub(crate) fn set_run_preview_state(
        &mut self,
        pinned: Option<RunState>,
        cx: &mut Context<Self>,
    ) {
        let run = &mut self.effects_ui.run;
        run.pinned = pinned;
        run.since = Some(std::time::Instant::now());
        run.viewed_at = None;
        cx.notify();
    }

    pub(crate) fn replay_run_preview(&mut self, cx: &mut Context<Self>) {
        self.set_run_preview_state(self.effects_ui.run.pinned, cx);
    }

    /// 点击大预览卡片：与画布上点击卡片一样，标记完成提示已查看。
    pub(crate) fn mark_run_preview_viewed(&mut self, cx: &mut Context<Self>) {
        self.effects_ui.run.viewed_at = Some(std::time::Instant::now());
        cx.notify();
    }

    pub(crate) fn set_run_preview_hover(
        &mut self,
        style: RunStyle,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        let hover = &mut self.effects_ui.run.hover;
        if entered {
            *hover = Some((style, std::time::Instant::now()));
        } else if hover.is_some_and(|(hovered, _)| hovered == style) {
            *hover = None;
        }
        cx.notify();
    }

    /// 乐观更新：先改缓存并重绘，后台写库。写入按序号串行：较旧的写入晚到时跳过，
    /// 库里总是最新的选择；只有最新一次保存失败才回滚到最后确认写入的值。
    fn save_run_style(
        &mut self,
        next: RunStyle,
        undo: Option<RunStyle>,
        label: String,
        cx: &mut Context<Self>,
    ) {
        let generation = with_effects(cx, |state| {
            state.run_generation += 1;
            state.run_style = next;
            state.run_undo = undo;
            state.run_status = Some(label);
            state.run_generation
        });
        self.notify_run_style_hosts(cx);
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        let work = cx
            .background_executor()
            .spawn(async move { write_latest(&settings, generation, next) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let failed = with_effects(cx, |state| match &result {
                    Ok(true) if generation > state.run_saved_generation => {
                        state.run_saved = next;
                        state.run_saved_generation = generation;
                        false
                    }
                    Ok(_) => false,
                    Err(error) => {
                        tracing::warn!(%error, style = next.id(), "run effect style save failed");
                        if generation != state.run_generation {
                            return false;
                        }
                        state.run_style = state.run_saved;
                        state.run_undo = None;
                        state.run_status = Some(ui::i18n::text("effects.save_failed").to_owned());
                        true
                    }
                });
                if failed {
                    app.notify_run_style_hosts(cx);
                }
            });
        })
        .detach();
    }
}

/// 进程内最后一次写入的序号；写入在锁内串行，较旧的写入直接跳过。
static LAST_WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

/// 返回是否真的写入（较旧的序号会被跳过）。
fn write_latest(
    settings: &storage::SettingsStore,
    generation: u64,
    style: RunStyle,
) -> storage::StorageResult<bool> {
    let mut last = LAST_WRITTEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if generation < *last {
        return Ok(false);
    }
    settings.set_string(KEY_RUN_STYLE, style.id())?;
    *last = generation;
    Ok(true)
}
