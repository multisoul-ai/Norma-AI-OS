use super::{components::VoiceAction, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use agent::voice_command::{
    plan_voice_command, resolve_voice_target, validate_voice_proposals, VoicePlan,
    VoiceTargetResolution,
};
use agent::AgentRosterEntry;
use gpui::{div, prelude::*, px, rgba, AppContext, Context, Entity, Render, Subscription, Window};
use std::path::PathBuf;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{InputEvent, InputState},
    Disableable,
};
use ui::Theme;

pub(crate) struct CommandPreview {
    theme: Theme,
    width: f32,
    input: Entity<InputState>,
    project: Option<PathBuf>,
    roster: Vec<AgentRosterEntry>,
    analyzed: Option<String>,
    _subscription: Subscription,
}

impl CommandPreview {
    fn new(theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| {
                InputState::new(window, cx).multi_line(true).placeholder(
                    ui::i18n::input_placeholder("voice.command_preview_placeholder", window, cx),
                )
            });
        let subscription = cx.subscribe(&input, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.analyzed = None;
                cx.notify();
            }
        });
        Self {
            theme,
            width: 280.0,
            input,
            project: None,
            roster: Vec::new(),
            analyzed: None,
            _subscription: subscription,
        }
    }

    pub(super) fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        // Re-render after a resize so empty component Inputs do not retain intrinsic widths.
        if (self.width - width).abs() > 0.5 {
            self.width = width.max(1.0);
            cx.notify();
        }
    }

    pub(super) fn target_summary(&self) -> String {
        let labels: Vec<_> = self
            .roster
            .iter()
            .filter(|m| m.enabled && m.active_node_id.is_some())
            .map(|m| format!("{} #{}", m.name, m.active_node_id.unwrap_or_default()))
            .collect();
        if labels.is_empty() {
            ui::i18n::text("voice.command_empty_canvas").to_string()
        } else {
            labels.join(" / ")
        }
    }

    fn result(&self, cx: &Context<Self>) -> (&'static str, String) {
        let value = self.input.read(cx).value();
        if self.analyzed.as_deref() != Some(value.as_str()) {
            return ("voice.command_preview_pending", String::new());
        }
        let target = match resolve_voice_target(&value, &self.roster) {
            VoiceTargetResolution::CanvasAgent {
                definition_id,
                node_id,
            } => self
                .roster
                .iter()
                .find(|m| m.definition_id == definition_id && m.active_node_id == Some(node_id))
                .map(|m| format!("{} #{node_id}", m.name))
                .unwrap_or_default(),
            VoiceTargetResolution::Assistant => "Norma".to_string(),
            VoiceTargetResolution::OutsideCanvas { .. } => {
                ui::i18n::text("voice.command_preview_outside").to_string()
            }
            VoiceTargetResolution::Ambiguous | VoiceTargetResolution::Unspecified => {
                ui::i18n::text("voice.command_ask_target").to_string()
            }
        };
        let action = match plan_voice_command(&value, &self.roster) {
            VoicePlan::Proposals(proposals) => {
                match validate_voice_proposals(&value, &self.roster, proposals) {
                    Ok(commits) if commits.len() == 1 => {
                        match commits[0].proposal().tool.as_str() {
                            "agent_assign" => "voice.command_preview_assign",
                            "agent_reply" => "voice.command_preview_reply",
                            _ => "voice.command_preview_model",
                        }
                    }
                    Ok(_) | Err(_) => "voice.command_preview_model",
                }
            }
            VoicePlan::NeedsModel => "voice.command_preview_model",
        };
        (action, target)
    }
}

impl Render for CommandPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let (action, target) = self.result(cx);
        let variant = ButtonCustomVariant::new(cx)
            .color(rgba(with_alpha(theme.accent_blue, 0x20)).into())
            .foreground(rgba(theme.accent_blue).into())
            .border(rgba(with_alpha(theme.accent_blue, 0x50)).into())
            .hover(rgba(with_alpha(theme.accent_blue, 0x30)).into())
            .active(rgba(with_alpha(theme.accent_blue, 0x40)).into());
        div()
            .id("voice-command-preview")
            .debug_selector(|| "voice-command-preview".into())
            .w(px(self.width))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(18.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
            .bg(rgba(theme.panel_high))
            .child(
                div()
                    .settings_text(TextRole::Label, theme)
                    .child(ui::i18n::text("voice.command_preview_local")),
            )
            .child(
                div()
                    .settings_text(TextRole::Headline, theme)
                    .child(ui::i18n::text("voice.command_preview_title")),
            )
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::text("voice.command_preview_hint")),
            )
            .child(
                crate::settings::norma::settings_editor(
                    "voice-command-preview-input",
                    &self.input,
                    false,
                    Some(72.0),
                    cx,
                )
                .debug_selector(|| "voice-command-preview-input".into()),
            )
            .child(
                Button::new("voice-command-preview-analyze")
                    .debug_selector(|| "voice-command-preview-analyze".into())
                    .custom(variant)
                    .label(ui::i18n::text("voice.command_preview_analyze"))
                    .disabled(self.input.read(cx).value().trim().is_empty())
                    .on_activate(cx.listener(|view, _, _, cx| {
                        view.analyzed = Some(view.input.read(cx).value().to_string());
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("voice-command-preview-result")
                    .debug_selector(move || format!("voice-command-preview-result-{action}"))
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .py(px(12.0))
                    .border_t_1()
                    .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
                    .child(
                        div()
                            .settings_text(TextRole::RowTitle, theme)
                            .child(ui::i18n::text(action)),
                    )
                    .when(!target.is_empty(), |result| {
                        result.child(div().settings_text(TextRole::Helper, theme).child(target))
                    }),
            )
            .child(
                div()
                    .settings_text(TextRole::Label, theme)
                    .child(ui::i18n::text("voice.command_current_canvas")),
            )
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(self.target_summary()),
            )
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::text("voice.command_preview_no_execution")),
            )
    }
}

impl VoiceSettingsInputs {
    pub(crate) fn prepare_command_preview(
        &mut self,
        theme: Theme,
        project: Option<PathBuf>,
        roster: Vec<AgentRosterEntry>,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) {
        let view = self
            .command_preview
            .get_or_insert_with(|| cx.new(|cx| CommandPreview::new(theme, window, cx)));
        view.update(cx, |view, _| {
            if view.project != project || view.roster != roster {
                view.analyzed = None;
            }
            view.theme = theme;
            view.project = project;
            view.roster = roster;
        });
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_command_preview_view_tests.rs"]
mod tests;
