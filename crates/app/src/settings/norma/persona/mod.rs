#[cfg(test)]
#[path = "../../../../tests/unit/persona_language_layout_tests.rs"]
mod language_layout_tests;
mod view;
use agent::{NormaPersona, PersonaTone};
use gpui::{App, AppContext, Context, Entity, Subscription, Window};
use storage::SettingsStore;
use ui::component::input::{InputEvent, InputState};

pub(crate) const PERSONA_SETTING_KEY: &str = "norma.persona.profile";

pub(crate) fn load_profile(store: &SettingsStore) -> Result<NormaPersona, String> {
    let profile: NormaPersona = store
        .get_string(PERSONA_SETTING_KEY)
        .map_err(|e| e.to_string())?
        .map(|text| {
            serde_json::from_str(&text).map_err(|e| {
                ui::i18n::format("norma.could_not_load_persona_e", &[("e", (e).to_string())])
            })
        })
        .transpose()?
        .unwrap_or_default();
    profile.validate()?;
    Ok(profile)
}

fn save_profile(store: &SettingsStore, profile: &NormaPersona) -> Result<(), String> {
    profile.validate()?;
    store
        .set_string(
            PERSONA_SETTING_KEY,
            &serde_json::to_string(profile).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
}

pub(crate) struct PersonaView {
    theme: ui::Theme,
    store: SettingsStore,
    saved: NormaPersona,
    tone: PersonaTone,
    instructions: Entity<InputState>,
    busy: bool,
    loaded: bool,
    error: bool,
    status: ui::i18n::LocalizedText,
    pub(crate) close_prompt: bool,
    _subscription: Subscription,
}

impl PersonaView {
    fn new(
        theme: ui::Theme,
        store: SettingsStore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let instructions = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .placeholder(ui::i18n::input_placeholder(
                    "norma.e_g_use_chinese_start_with_the_conclusion_when_discussing_technical",
                    window,
                    cx,
                ))
        });
        let subscription = cx.subscribe(&instructions, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.status.clear();
                view.error = false;
                cx.notify();
            }
        });
        let mut view = Self {
            theme,
            store,
            saved: Default::default(),
            tone: Default::default(),
            instructions,
            busy: false,
            loaded: false,
            error: false,
            status: ui::i18n::LocalizedText::default(),
            close_prompt: false,
            _subscription: subscription,
        };
        view.reload(window, cx);
        view
    }
    fn draft(&self, cx: &App) -> NormaPersona {
        NormaPersona {
            tone: self.tone,
            instructions: self.instructions.read(cx).value().to_string(),
        }
    }
    pub(crate) fn is_dirty(&self, cx: &App) -> bool {
        self.loaded && (self.busy || self.draft(cx) != self.saved)
    }
    fn replace(&mut self, profile: NormaPersona, window: &mut Window, cx: &mut Context<Self>) {
        self.tone = profile.tone;
        self.instructions.update(cx, |input, cx| {
            input.set_value(profile.instructions, window, cx)
        });
    }
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = ui::i18n::LocalizedText::new("common.loading");
        let store = self.store.clone();
        let task = cx
            .background_executor()
            .spawn(async move { load_profile(&store) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok(profile) => {
                        view.saved = profile.clone();
                        view.replace(profile, window, cx);
                        view.loaded = true;
                        view.error = false;
                        view.status.clear();
                    }
                    Err(error) => {
                        view.error = true;
                        view.status =
                            ui::i18n::LocalizedText::new("norma.could_not_load_persona_e")
                                .with_arg("e", error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.loaded {
            return;
        }
        let profile = self.draft(cx);
        if let Some(error) = persona_validation(&profile) {
            self.error = true;
            self.status = error;
            cx.notify();
            return;
        }
        self.busy = true;
        self.status = ui::i18n::LocalizedText::new("norma.saving");
        let store = self.store.clone();
        let task = cx.background_executor().spawn(async move {
            save_profile(&store, &profile)?;
            Ok::<_, String>(profile)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(profile) => {
                        view.saved = profile;
                        view.close_prompt = false;
                        view.error = false;
                        view.status = ui::i18n::LocalizedText::new(
                            "norma.saved_applies_to_the_next_conversation_turn",
                        );
                    }
                    Err(error) => {
                        view.error = true;
                        view.status = ui::i18n::message(
                            "norma.save_failed_your_changes_are_preserved_error",
                            &[("error", (error).to_string())],
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.replace(self.saved.clone(), window, cx);
        self.close_prompt = false;
        self.status.clear();
        self.error = false;
        cx.notify();
    }
}

impl crate::app::MultisoulApp {
    pub(crate) fn ensure_norma_persona(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.norma_persona.is_none() {
            let store = SettingsStore::new(self.database_paths.app_db_path().to_path_buf());
            let view = cx.new(|cx| PersonaView::new(self.theme, store, window, cx));
            cx.observe(&view, |_, _, cx| cx.notify()).detach();
            self.norma_persona = Some(view);
        }
        if let Some(view) = &self.norma_persona {
            view.update(cx, |view, _| view.theme = self.theme);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/settings_persona_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/unit/settings_input_routing_tests.rs"]
mod input_routing_tests;

fn tone_text(tone: PersonaTone, field: &str) -> &'static str {
    let key = match (tone, field) {
        (PersonaTone::Warm, "label") => "persona.warm.label",
        (PersonaTone::Professional, "label") => "persona.professional.label",
        (PersonaTone::Concise, "label") => "persona.concise.label",
        (PersonaTone::Warm, "description") => "persona.warm.description",
        (PersonaTone::Professional, "description") => "persona.professional.description",
        (PersonaTone::Concise, "description") => "persona.concise.description",
        (PersonaTone::Warm, _) => "persona.warm.example",
        (PersonaTone::Professional, _) => "persona.professional.example",
        (PersonaTone::Concise, _) => "persona.concise.example",
    };
    ui::i18n::text(key)
}

fn persona_validation(persona: &NormaPersona) -> Option<ui::i18n::LocalizedText> {
    if persona.instructions.chars().count() > agent::MAX_PERSONA_INSTRUCTION_CHARS {
        Some(
            ui::i18n::LocalizedText::new("persona.instructions_too_long")
                .with_arg("limit", agent::MAX_PERSONA_INSTRUCTION_CHARS),
        )
    } else if persona
        .instructions
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        Some(ui::i18n::LocalizedText::new("persona.instructions_control"))
    } else {
        None
    }
}
