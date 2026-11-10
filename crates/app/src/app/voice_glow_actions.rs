//! 语音光晕偏好的保存、撤销与预览状态。全局偏好，存在 `effects.voice_glow.*`；
//! 渲染与光晕总线只读 gpui Global 缓存，不碰 SQLite。

use crate::app::effects_actions::with_effects;
use crate::app::MultisoulApp;
use crate::effects::voice_glow::preview::PreviewPin;
use crate::effects::voice_glow::{self, Gain, VoiceGlowPrefs, VoiceGlowStyle};
use crate::settings::effects::state::EffectsSettingsState;
use gpui::{App, Context};
use std::time::Instant;

pub(crate) fn voice_glow_prefs(cx: &App) -> VoiceGlowPrefs {
    cx.try_global::<EffectsSettingsState>()
        .map(|state| state.voice_glow)
        .unwrap_or_default()
}

pub(crate) fn style_name(style: VoiceGlowStyle) -> &'static str {
    ui::i18n::text(match style {
        VoiceGlowStyle::Off => "effects.glow_style_off",
        VoiceGlowStyle::Classic => "effects.glow_style_classic",
        VoiceGlowStyle::Halo => "effects.glow_style_halo",
        VoiceGlowStyle::Ribbon => "effects.glow_style_ribbon",
        VoiceGlowStyle::Stardust => "effects.glow_style_stardust",
    })
}

pub(crate) fn style_line(style: VoiceGlowStyle) -> &'static str {
    ui::i18n::text(match style {
        VoiceGlowStyle::Off => "effects.glow_line_off",
        VoiceGlowStyle::Classic => "effects.glow_line_classic",
        VoiceGlowStyle::Halo => "effects.glow_line_halo",
        VoiceGlowStyle::Ribbon => "effects.glow_line_ribbon",
        VoiceGlowStyle::Stardust => "effects.glow_line_stardust",
    })
}

impl MultisoulApp {
    pub(crate) fn pick_voice_glow_style(&mut self, style: VoiceGlowStyle, cx: &mut Context<Self>) {
        let previous = voice_glow_prefs(cx);
        if previous.style == style {
            return;
        }
        let label = ui::i18n::format(
            "effects.glow_saved",
            &[("style", style_name(style).to_owned())],
        );
        self.save_voice_glow(
            VoiceGlowPrefs { style, ..previous },
            Some(previous),
            label,
            cx,
        );
    }

    pub(crate) fn set_voice_glow_gain(&mut self, gain: Gain, cx: &mut Context<Self>) {
        let previous = voice_glow_prefs(cx);
        if previous.gain != gain {
            let label = ui::i18n::text("effects.glow_options_saved").to_owned();
            self.save_voice_glow(
                VoiceGlowPrefs { gain, ..previous },
                Some(previous),
                label,
                cx,
            );
        }
    }

    pub(crate) fn toggle_voice_glow_phases(&mut self, cx: &mut Context<Self>) {
        let previous = voice_glow_prefs(cx);
        let label = ui::i18n::text("effects.glow_options_saved").to_owned();
        let next = VoiceGlowPrefs {
            show_listening_thinking: !previous.show_listening_thinking,
            ..previous
        };
        self.save_voice_glow(next, Some(previous), label, cx);
    }

    pub(crate) fn undo_voice_glow(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = with_effects(cx, |state| state.glow_undo.take()) else {
            return;
        };
        let label = ui::i18n::format(
            "effects.glow_saved",
            &[("style", style_name(previous.style).to_owned())],
        );
        self.save_voice_glow(previous, None, label, cx);
    }

    /// 预览固定某个状态；`None` 恢复自动轮播。都从头计时。
    pub(crate) fn set_glow_preview_pin(&mut self, pin: Option<PreviewPin>, cx: &mut Context<Self>) {
        let glow = &mut self.effects_ui.glow;
        glow.pin = pin;
        glow.since = Some(Instant::now());
        cx.notify();
    }

    pub(crate) fn replay_glow_preview(&mut self, cx: &mut Context<Self>) {
        self.set_glow_preview_pin(self.effects_ui.glow.pin, cx);
    }

    pub(crate) fn set_glow_preview_hover(
        &mut self,
        style: VoiceGlowStyle,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        let hover = &mut self.effects_ui.glow.hover;
        if entered {
            *hover = Some((style, Instant::now()));
        } else if hover.is_some_and(|(hovered, _)| hovered == style) {
            *hover = None;
        }
        cx.notify();
    }

    /// 乐观更新：先改缓存并重绘，后台写库。写入按序号串行，较旧的写入晚到时跳过；
    /// 只有最新一次保存失败才回滚到最后确认写入的值。
    fn save_voice_glow(
        &mut self,
        next: VoiceGlowPrefs,
        undo: Option<VoiceGlowPrefs>,
        label: String,
        cx: &mut Context<Self>,
    ) {
        let generation = with_effects(cx, |state| {
            state.glow_generation += 1;
            state.voice_glow = next;
            state.glow_undo = undo;
            state.glow_status = Some(label);
            state.glow_generation
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
                    Ok(true) if generation > state.glow_saved_generation => {
                        state.glow_saved = next;
                        state.glow_saved_generation = generation;
                        false
                    }
                    Ok(_) => false,
                    Err(error) => {
                        tracing::warn!(%error, style = next.style.id(), "voice glow save failed");
                        if generation != state.glow_generation {
                            return false;
                        }
                        state.voice_glow = state.glow_saved;
                        state.glow_undo = None;
                        state.glow_status = Some(ui::i18n::text("effects.save_failed").to_owned());
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

fn write_latest(
    settings: &storage::SettingsStore,
    generation: u64,
    prefs: VoiceGlowPrefs,
) -> storage::StorageResult<bool> {
    let mut last = LAST_WRITTEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if generation < *last {
        return Ok(false);
    }
    settings.set_string(voice_glow::KEY_STYLE, prefs.style.id())?;
    settings.set_string(voice_glow::KEY_GAIN, prefs.gain.key())?;
    settings.set_string(
        voice_glow::KEY_PHASES,
        if prefs.show_listening_thinking {
            "true"
        } else {
            "false"
        },
    )?;
    *last = generation;
    Ok(true)
}
