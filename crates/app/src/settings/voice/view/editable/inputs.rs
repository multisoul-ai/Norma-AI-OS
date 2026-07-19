use super::{new_input, status, VoiceInputConfig, VoiceTextInput};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{App, Context, Entity, FocusHandle, WeakEntity, Window};
use ui::Theme;

pub(crate) struct VoiceSettingsInputs {
    base_url: Entity<VoiceTextInput>,
    model: Entity<VoiceTextInput>,
    pub(super) api_key: Entity<VoiceTextInput>,
    azure_region: Entity<VoiceTextInput>,
    azure_locale: Entity<VoiceTextInput>,
    azure_api_key: Entity<VoiceTextInput>,
}

impl VoiceSettingsInputs {
    pub(crate) fn load_default(
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        theme: Theme,
        cx: &mut Context<MultisoulApp>,
    ) -> (VoiceSettingsState, Self) {
        let state = VoiceSettingsState::load_default();
        let inputs = Self::new(app, root_focus, &state, theme, cx);
        (state, inputs)
    }

    pub(crate) fn new(
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        state: &VoiceSettingsState,
        theme: Theme,
        cx: &mut Context<MultisoulApp>,
    ) -> Self {
        let base_url = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::BaseUrl,
                value: state.editor_value(VoiceSettingsField::BaseUrl),
                placeholder: "https://api.example.com/v1",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let model = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::Model,
                value: state.editor_value(VoiceSettingsField::Model),
                placeholder: "Model name",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let api_key = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::ApiKey,
                value: state.editor_value(VoiceSettingsField::ApiKey),
                placeholder: status::input_placeholder(state, VoiceSettingsField::ApiKey),
                masked: true,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let azure_region = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::AzureRegion,
                value: state.editor_value(VoiceSettingsField::AzureRegion),
                placeholder: "eastus",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let azure_locale = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::AzureLocale,
                value: state.editor_value(VoiceSettingsField::AzureLocale),
                placeholder: "zh-CN",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let azure_api_key = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::AzureApiKey,
                value: state.editor_value(VoiceSettingsField::AzureApiKey),
                placeholder: status::input_placeholder(state, VoiceSettingsField::AzureApiKey),
                masked: true,
                theme,
            },
            app,
            root_focus,
            cx,
        );
        Self {
            base_url,
            model,
            api_key,
            azure_region,
            azure_locale,
            azure_api_key,
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
        ] {
            self.sync_field_from_state(field, state, cx);
        }
    }

    pub(crate) fn set_theme(&self, theme: Theme, cx: &mut Context<MultisoulApp>) {
        for field in [
            VoiceSettingsField::BaseUrl,
            VoiceSettingsField::Model,
            VoiceSettingsField::ApiKey,
            VoiceSettingsField::AzureRegion,
            VoiceSettingsField::AzureLocale,
            VoiceSettingsField::AzureApiKey,
        ] {
            self.input(field)
                .update(cx, |input, cx| input.set_theme(theme, cx));
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
