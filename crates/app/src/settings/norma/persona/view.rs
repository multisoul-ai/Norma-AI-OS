use super::*;
use crate::settings::spacing::{CARD_PADDING_COMPACT, ROW_GAP, SECTION_GAP, SECTION_GAP_COMPACT};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Render, Window};
use ui::component::StyledExt;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, Sizable,
};
use ui::icons::with_alpha;

fn caption(theme: ui::Theme, text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .min_w(px(0.0))
        .whitespace_normal()
        .settings_text(TextRole::Helper, theme)
        .child(text.into())
}

impl Render for PersonaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let draft = self.draft(cx);
        let validation = persona_validation(&draft);
        let disabled = self.busy || !self.loaded;
        div()
            .id("norma-persona")
            .debug_selector(|| "norma-persona".into())
            .w_full()
            .h_full()
            .min_h(px(0.0))
            .max_w(px(780.0))
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP_COMPACT))
            .settings_text(TextRole::Body, theme)
            .when(self.close_prompt, |page| {
                page.child(self.unsaved_prompt(cx))
            })
            .child(
                div()
                    .id("persona-content-scroll")
                    .debug_selector(|| "persona-content-scroll".into())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(SECTION_GAP_COMPACT))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_4()
                                    .child(
                                        div()
                                            .size(px(52.0))
                                            .flex_none()
                                            .rounded(px(16.0))
                                            .bg(rgba(with_alpha(theme.accent, 0x16)))
                                            .text_color(rgba(theme.accent))
                                            .text_size(px(25.0))
                                            .font_semibold()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child("N"),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().settings_font(TextRole::Headline).child("Norma"))
                                            .child(caption(
                                                theme,
                                                ui::i18n::text("norma.your_ai_companion_talk_and_coordinate_projects_and_agents"),
                                            ).id("persona-summary").debug_selector(|| "persona-summary".into())),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(ROW_GAP))
                                    .child(div().settings_font(TextRole::Headline).child(ui::i18n::text("norma.communication_style")))
                                    .child(caption(
                                        theme,
                                        ui::i18n::text("norma.choose_your_preferred_style_this_persona_applies_to_personal_and_project"),
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(ROW_GAP))
                                            .child(self.tone_card(PersonaTone::Warm, disabled, cx))
                                            .child(self.tone_card(PersonaTone::Professional, disabled, cx))
                                            .child(self.tone_card(PersonaTone::Concise, disabled, cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("persona-example")
                                    .debug_selector(|| "persona-example".into())
                                    .p(px(CARD_PADDING_COMPACT))
                                    .rounded(px(12.0))
                                    .bg(rgba(with_alpha(theme.accent, 0x0c)))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(caption(theme, ui::i18n::text("norma.example")))
                                    .child(div().settings_font(TextRole::Body).child(tone_text(self.tone, "example"))),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().settings_font(TextRole::Headline).child(ui::i18n::text("norma.additional_instructions")))
                                    .child(caption(
                                        theme,
                                        ui::i18n::text("norma.tell_norma_how_you_prefer_to_communicate_such_as_language_detail"),
                                    ))
                                    .child(crate::settings::norma::view::editor(
                                        "persona-instructions",
                                        &self.instructions,
                                        disabled,
                                        Some(100.0),
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .gap(px(ROW_GAP))
                                            .child(caption(
                                                theme,
                                                ui::i18n::text("norma.persona_shapes_communication_manage_memories_separately_on_the_memory_page"),
                                            ))
                                            .child(caption(
                                                theme,
                                                format!(
                                                    "{} / {}",
                                                    draft.instructions.chars().count(),
                                                    agent::MAX_PERSONA_INSTRUCTION_CHARS
                                                ),
                                            )),
                                    ),
                            )
                            .when(validation.is_some(), |page| {
                                page.child(
                                    div()
                                        .settings_font(TextRole::Helper)
                                        .text_color(rgba(theme.destructive))
                                        .child(validation.unwrap_or_default().to_string()),
                                )
                            })
                            .when(!self.status.is_empty(), |page| {
                                page.child(
                                    div()
                                        .id("persona-status")
                                        .debug_selector(|| "persona-status".into())
                                        .settings_font(TextRole::Helper)
                                        .text_color(rgba(if self.error {
                                            theme.destructive
                                        } else {
                                            theme.text_secondary
                                        }))
                                        .child(self.status.to_string()),
                                )
                            })
                            .when(!self.loaded && !self.busy, |page| {
                                page.child(
                                    Button::new("persona-reload")
                                        .small()
                                        .child(control_label(ui::i18n::text("norma.reload")))
                                        .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                                )
                            })
                    ),
            )
            .child(
                div()
                    .id("persona-actions")
                    .debug_selector(|| "persona-actions".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(ROW_GAP))
                    .pt(px(SECTION_GAP))
                    .border_t_1()
                    .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                    .child(
                        Button::new("persona-reset")
                            .ghost()
                            .small()
                            .child(control_label(ui::i18n::text("norma.restore_defaults")))
                            .disabled(disabled)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.replace(NormaPersona::default(), window, cx);
                                view.status = ui::i18n::LocalizedText::new("norma.default_persona_filled_in_save_to_apply");
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(ROW_GAP))
                            .child(caption(
                                theme,
                                if draft != self.saved {
                                    ui::i18n::text("norma.unsaved_changes")
                                } else {
                                    ui::i18n::text("norma.saved")
                                },
                            ))
                            .child(
                                Button::new("persona-cancel")
                                    .ghost()
                                    .small()
                                    .child(control_label(ui::i18n::text("common.cancel")))
                                    .disabled(disabled || draft == self.saved)
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.discard(window, cx)),
                                    ),
                            )
                            .child(
                                Button::new("persona-save")
                                    .primary()
                                    .small()
                                    .child(control_label(if self.busy && self.loaded {
                                        ui::i18n::text("norma.saving")
                                    } else {
                                        ui::i18n::text("common.save")
                                    }))
                                    .disabled(
                                        disabled
                                            || draft == self.saved
                                            || draft.validate().is_err(),
                                    )
                                    .on_click(cx.listener(|view, _, _, cx| view.save(cx))),
                            ),
                    ),
            )
    }
}

impl PersonaView {
    fn unsaved_prompt(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("persona-unsaved-prompt")
            .debug_selector(|| "persona-unsaved-prompt".into())
            .p_4()
            .rounded(px(12.0))
            .bg(rgba(with_alpha(self.theme.accent_orange, 0x12)))
            .child(
                div()
                    .settings_font(TextRole::Body)
                    .child(ui::i18n::text("norma.your_persona_has_unsaved_changes")),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("persona-discard")
                            .small()
                            .child(control_label(ui::i18n::text("norma.discard_changes")))
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| view.discard(window, cx))),
                    )
                    .child(
                        Button::new("persona-continue")
                            .ghost()
                            .small()
                            .child(control_label(ui::i18n::text("norma.keep_editing")))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.close_prompt = false;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn tone_card(
        &self,
        tone: PersonaTone,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let selected = self.tone == tone;
        let theme = self.theme;
        let variant = ButtonCustomVariant::new(cx)
            .color(
                rgba(if selected {
                    with_alpha(theme.accent, 0x14)
                } else {
                    theme.panel_high
                })
                .into(),
            )
            .foreground(rgba(theme.text_primary).into())
            .border(
                rgba(if selected {
                    with_alpha(theme.accent, 0xa0)
                } else {
                    with_alpha(theme.text_secondary, 0x25)
                })
                .into(),
            )
            .hover(rgba(with_alpha(theme.accent, 0x22)).into())
            .active(rgba(with_alpha(theme.accent, 0x35)).into());
        div()
            .id(gpui::ElementId::Name(
                format!("persona-tone-{tone:?}").into(),
            ))
            .debug_selector(move || format!("persona-tone-{tone:?}"))
            .relative()
            .flex_1()
            .min_w(px(160.0))
            .min_h(px(96.0))
            .child(
                Button::new(gpui::ElementId::Name(
                    format!("persona-tone-button-{tone:?}").into(),
                ))
                .absolute()
                .inset_0()
                .size_full()
                .custom(variant)
                .disabled(disabled)
                .tooltip(tone_text(tone, "label"))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.tone = tone;
                    view.error = false;
                    view.status.clear();
                    cx.notify();
                })),
            )
            .child(
                // Keep wrapping content outside Button's intrinsic label row.
                div()
                    .relative()
                    .p(px(CARD_PADDING_COMPACT))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .settings_font(TextRole::RowTitle)
                                    .child(tone_text(tone, "label")),
                            )
                            .child(div().text_color(rgba(theme.accent)).child(if selected {
                                "✓"
                            } else {
                                ""
                            })),
                    )
                    .child(
                        caption(theme, tone_text(tone, "description"))
                            .id(gpui::ElementId::Name(
                                format!("persona-tone-description-{tone:?}").into(),
                            ))
                            .debug_selector(move || format!("persona-tone-description-{tone:?}")),
                    ),
            )
    }
}
