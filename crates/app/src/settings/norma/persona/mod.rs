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
        .map(|text| serde_json::from_str(&text).map_err(|e| format!("人设配置无法读取：{e}")))
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
    status: String,
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
                .placeholder("例如：用中文交流；讨论技术时先说结论，再解释原因。")
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
            status: String::new(),
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
        self.status = "正在载入…".into();
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
                        view.status = error;
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
        if let Err(error) = profile.validate() {
            self.error = true;
            self.status = error;
            cx.notify();
            return;
        }
        self.busy = true;
        self.status = "正在保存…".into();
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
                        view.status = "已保存，下一轮交流生效".into();
                    }
                    Err(error) => {
                        view.error = true;
                        view.status = format!("保存失败，修改已保留：{error}");
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
