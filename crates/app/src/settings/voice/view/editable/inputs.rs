use super::{new_input, status, VoiceInputConfig, VoiceTextInput};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{App, Context, Entity, FocusHandle, WeakEntity, Window};
use ui::Theme;

#[derive(Clone, Copy)]
struct VoiceInputSpec {
    field: VoiceSettingsField,
    placeholder: &'static str,
    masked: bool,
}

fn create_input(
    spec: VoiceInputSpec,
    app: WeakEntity<MultisoulApp>,
    root_focus: FocusHandle,
    state: &VoiceSettingsState,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Entity<VoiceTextInput> {
    let VoiceInputSpec {
        field,
        placeholder,
        masked,
    } = spec;
    new_input(
        VoiceInputConfig {
            field,
            value: state.editor_value(field),
            placeholder,
            masked,
            theme,
        },
        app,
        root_focus,
        cx,
    )
}

pub(crate) struct VoiceSettingsInputs {
    base_url: Entity<VoiceTextInput>,
    model: Entity<VoiceTextInput>,
    pub(super) api_key: Entity<VoiceTextInput>,
    azure_region: Entity<VoiceTextInput>,
    azure_locale: Entity<VoiceTextInput>,
    azure_api_key: Entity<VoiceTextInput>,
    minimax_api_key: Entity<VoiceTextInput>,
    minimax_voice_id: Entity<VoiceTextInput>,
    minimax_voice_search: Entity<VoiceTextInput>,
    doubao_api_key: Entity<VoiceTextInput>,
    doubao_voice_search: Entity<VoiceTextInput>,
}

impl VoiceSettingsInputs {
    pub(crate) fn new(
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        state: &VoiceSettingsState,
        theme: Theme,
        cx: &mut Context<MultisoulApp>,
    ) -> Self {
        let base_url = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::BaseUrl,
                placeholder: "https://api.example.com/v1",
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let model = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::Model,
                placeholder: ui::i18n::text("voice.model_name"),
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let api_key = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::ApiKey,
                placeholder: status::input_placeholder(state, VoiceSettingsField::ApiKey),
                masked: true,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let azure_region = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::AzureRegion,
                placeholder: "eastus",
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let azure_locale = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::AzureLocale,
                placeholder: "zh-CN",
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let azure_api_key = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::AzureApiKey,
                placeholder: status::input_placeholder(state, VoiceSettingsField::AzureApiKey),
                masked: true,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let minimax_api_key = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::MiniMaxApiKey,
                placeholder: status::input_placeholder(state, VoiceSettingsField::MiniMaxApiKey),
                masked: true,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let minimax_voice_id = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::MiniMaxVoiceId,
                placeholder: ui::i18n::text("voice.enter_voice_id"),
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let minimax_voice_search = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::MiniMaxVoiceSearch,
                placeholder: ui::i18n::text("voice.search_by_name_or_voice_id"),
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let doubao_api_key = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::DoubaoApiKey,
                placeholder: status::input_placeholder(state, VoiceSettingsField::DoubaoApiKey),
                masked: true,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        let doubao_voice_search = create_input(
            VoiceInputSpec {
                field: VoiceSettingsField::DoubaoVoiceSearch,
                placeholder: ui::i18n::text("doubao.search_voices"),
                masked: false,
            },
            app.clone(),
            root_focus.clone(),
            state,
            theme,
            cx,
        );
        Self {
            base_url,
            model,
            api_key,
            azure_region,
            azure_locale,
            azure_api_key,
            minimax_api_key,
            minimax_voice_id,
            minimax_voice_search,
            doubao_api_key,
            doubao_voice_search,
        }
    }

    pub(crate) fn input(&self, field: VoiceSettingsField) -> Entity<VoiceTextInput> {
        match field {
            VoiceSettingsField::BaseUrl => self.base_url.clone(),
            VoiceSettingsField::Model => self.model.clone(),
            VoiceSettingsField::ApiKey => self.api_key.clone(),
            VoiceSettingsField::AzureRegion => self.azure_region.clone(),
            VoiceSettingsField::AzureLocale => self.azure_locale.clone(),
            VoiceSettingsField::AzureApiKey => self.azure_api_key.clone(),
            VoiceSettingsField::MiniMaxApiKey => self.minimax_api_key.clone(),
            VoiceSettingsField::MiniMaxVoiceId => self.minimax_voice_id.clone(),
            VoiceSettingsField::MiniMaxVoiceSearch => self.minimax_voice_search.clone(),
            VoiceSettingsField::DoubaoApiKey => self.doubao_api_key.clone(),
            VoiceSettingsField::DoubaoVoiceSearch => self.doubao_voice_search.clone(),
        }
    }

    pub(crate) fn sync_from_state(
        &self,
        state: &VoiceSettingsState,
        cx: &mut Context<MultisoulApp>,
    ) {
        for field in [
            VoiceSettingsField::BaseUrl,
            VoiceSettingsField::Model,
            VoiceSettingsField::ApiKey,
            VoiceSettingsField::AzureRegion,
            VoiceSettingsField::AzureLocale,
            VoiceSettingsField::AzureApiKey,
            VoiceSettingsField::MiniMaxApiKey,
            VoiceSettingsField::MiniMaxVoiceId,
            VoiceSettingsField::MiniMaxVoiceSearch,
            VoiceSettingsField::DoubaoApiKey,
            VoiceSettingsField::DoubaoVoiceSearch,
        ] {
            self.sync_field_from_state(field, state, cx);
        }
    }

    pub(crate) fn set_theme(&self, theme: Theme, cx: &mut Context<MultisoulApp>) {
        for input in [
            &self.base_url,
            &self.model,
            &self.api_key,
            &self.azure_region,
            &self.azure_locale,
            &self.azure_api_key,
            &self.minimax_api_key,
            &self.minimax_voice_id,
            &self.minimax_voice_search,
            &self.doubao_api_key,
            &self.doubao_voice_search,
        ] {
            input.update(cx, |input, cx| input.set_theme(theme, cx));
        }
    }

    pub(crate) fn is_focused(&self, window: &Window, cx: &App) -> bool {
        [
            &self.base_url,
            &self.model,
            &self.api_key,
            &self.azure_region,
            &self.azure_locale,
            &self.azure_api_key,
            &self.minimax_api_key,
            &self.minimax_voice_id,
            &self.minimax_voice_search,
            &self.doubao_api_key,
            &self.doubao_voice_search,
        ]
        .into_iter()
        .any(|input| input.read(cx).focus_handle.is_focused(window))
    }

    pub(crate) fn input_surface_size(
        &self,
        field: VoiceSettingsField,
        cx: &App,
    ) -> Option<(u32, u32)> {
        self.input(field).read(cx).last_bounds.map(|bounds| {
            let width = (bounds.size.width / gpui::px(1.0)).max(0.0).round() as u32;
            let height = (bounds.size.height / gpui::px(1.0)).max(0.0).round() as u32;
            (width, height)
        })
    }

    pub(crate) fn rendered_text(&self, field: VoiceSettingsField, cx: &App) -> Option<String> {
        self.input(field)
            .read(cx)
            .last_layout
            .as_ref()
            .map(|layout| layout.text.to_string())
    }
}
