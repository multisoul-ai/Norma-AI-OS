use super::{action_button, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{TtsModelInstallState, VoiceSettingsState};
use gpui::{
    div, prelude::*, px, rgba, AppContext, Context, Div, Entity, Focusable, Subscription, Window,
};
use ui::component::input::{Input, InputEvent, InputState};
use ui::Theme;

#[derive(Clone, Copy)]
enum ReferenceField {
    Path,
    Transcript,
}

struct ReferenceFieldState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub(super) fn zipvoice_reference_controls(
    theme: Theme,
    state: &VoiceSettingsState,
    descriptor: &tts::TtsModelDescriptor,
    installation: &TtsModelInstallState,
    model_index: usize,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Option<Div> {
    if descriptor.backend() != tts::TtsBackend::ZipVoice
        || !matches!(installation, TtsModelInstallState::Ready)
    {
        return None;
    }
    let path = reference_input(
        ReferenceField::Path,
        state.zipvoice_reference_draft_path(),
        state.zipvoice_reference_input_generation(),
        model_index,
        window,
        cx,
    );
    let transcript = reference_input(
        ReferenceField::Transcript,
        state.zipvoice_reference_draft_transcript(),
        state.zipvoice_reference_input_generation(),
        model_index,
        window,
        cx,
    );
    let path_focus = path.focus_handle(cx);
    let transcript_focus = transcript.focus_handle(cx);
    let ready = state.zipvoice_reference_ready();
    let importing = state.zipvoice_reference_importing();
    let busy = state.zipvoice_reference_operation_busy();
    let path_present = !state.zipvoice_reference_draft_path().trim().is_empty();
    let transcript_present = !state
        .zipvoice_reference_draft_transcript()
        .trim()
        .is_empty();
    let transcript_required = path_present && !transcript_present;
    let can_import = state.zipvoice_reference_can_import();
    let import_selector = if can_import {
        "settings-zipvoice-reference-import"
    } else {
        "settings-zipvoice-reference-import-disabled"
    };
    let mut controls =
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pt(px(4.0))
            .border_t_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x35)))
            .child(
                div()
                    .pt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child("Clone reference · 16 kHz mono PCM WAV · 3–15 seconds"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(reference_field_label(theme, "Reference WAV"))
                    .child(
                        div()
                            .id("settings-zipvoice-reference-path")
                            .debug_selector(|| "settings-zipvoice-reference-path".to_string())
                            .w_full()
                            .min_h(px(38.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgba(with_alpha(theme.edge_highlight, 0x58)))
                            .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
                            .on_click(cx.listener(move |_, _, window, _| path_focus.focus(window)))
                            .child(Input::new(&path).appearance(false).cleanable(true)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(reference_field_label(theme, "Exact transcript"))
                    .child(
                        div()
                            .id("settings-zipvoice-reference-transcript")
                            .debug_selector(|| "settings-zipvoice-reference-transcript".to_string())
                            .w_full()
                            .min_h(px(38.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgba(if transcript_required {
                                with_alpha(theme.destructive, 0x98)
                            } else {
                                with_alpha(theme.edge_highlight, 0x58)
                            }))
                            .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
                            .on_click(
                                cx.listener(move |_, _, window, _| transcript_focus.focus(window)),
                            )
                            .child(Input::new(&transcript).appearance(false).cleanable(true)),
                    )
                    .when(transcript_required, |field| {
                        field.child(
                            div()
                                .id("settings-zipvoice-reference-transcript-required")
                                .debug_selector(|| {
                                    "settings-zipvoice-reference-transcript-required".to_string()
                                })
                                .text_size(px(11.0))
                                .text_color(rgba(theme.destructive))
                                .child("Required — enter the exact words spoken in the WAV."),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        action_button(theme, "Choose WAV…", theme.accent_blue)
                            .id("settings-zipvoice-reference-browse")
                            .debug_selector(|| "settings-zipvoice-reference-browse".to_string())
                            .when(!busy, |button| {
                                button.on_click(cx.listener(|app, _, _, cx| {
                                    app.choose_zipvoice_reference_audio(cx)
                                }))
                            }),
                    )
                    .child(
                        action_button(
                            theme,
                            if importing {
                                "Importing…"
                            } else if ready {
                                "Replace"
                            } else {
                                "Import"
                            },
                            theme.accent_blue,
                        )
                        .id(import_selector)
                        .debug_selector(move || import_selector.to_string())
                        .when(!can_import, |button| button.opacity(0.48).cursor_default())
                        .when(can_import, |button| {
                            button.on_click(
                                cx.listener(|app, _, _, cx| app.import_zipvoice_reference(cx)),
                            )
                        }),
                    ),
            );
    if ready {
        controls = controls.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .text_color(rgba(theme.accent_green))
                .child("Reference ready")
                .child(
                    action_button(theme, "Remove", theme.destructive)
                        .id("settings-zipvoice-reference-remove")
                        .debug_selector(|| "settings-zipvoice-reference-remove".to_string())
                        .when(!busy, |button| {
                            button.on_click(
                                cx.listener(|app, _, _, cx| app.remove_zipvoice_reference(cx)),
                            )
                        }),
                ),
        );
    }
    Some(controls)
}

fn reference_field_label(theme: Theme, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(px(11.0))
        .text_color(rgba(theme.text_secondary))
        .child(label)
        .child(div().text_color(rgba(theme.destructive)).child("Required"))
}

fn reference_input(
    field: ReferenceField,
    initial_value: &str,
    generation: u64,
    model_index: usize,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Entity<InputState> {
    let field_index = match field {
        ReferenceField::Path => 0,
        ReferenceField::Transcript => 1,
    };
    let app = cx.weak_entity();
    let initial_value = initial_value.to_string();
    let key = ((model_index as u64) << 48) ^ ((field_index as u64) << 40) ^ generation;
    let state = window.use_keyed_state(
        ("settings-zipvoice-reference-input", key),
        cx,
        move |window, cx| {
            let placeholder = match field {
                ReferenceField::Path => "/path/to/reference.wav",
                ReferenceField::Transcript => "Exact transcript of the reference audio",
            };
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .multi_line(false)
                    .default_value(initial_value)
            });
            let subscription = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                let Some(app) = app.upgrade() else {
                    return;
                };
                let value = input.read(cx).value().replace(['\r', '\n'], "");
                app.update(cx, |app, cx| match event {
                    InputEvent::Change => {
                        match field {
                            ReferenceField::Path => app
                                .voice_settings
                                .set_zipvoice_reference_draft_path(value, false),
                            ReferenceField::Transcript => app
                                .voice_settings
                                .set_zipvoice_reference_draft_transcript(value),
                        }
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => {
                        match field {
                            ReferenceField::Path => app
                                .voice_settings
                                .set_zipvoice_reference_draft_path(value, false),
                            ReferenceField::Transcript => app
                                .voice_settings
                                .set_zipvoice_reference_draft_transcript(value),
                        }
                        app.voice_settings
                            .reset_zipvoice_reference_inputs_for_submit();
                        app.import_zipvoice_reference(cx);
                    }
                    InputEvent::Focus | InputEvent::Blur => {}
                });
            });
            ReferenceFieldState {
                input,
                _subscription: subscription,
            }
        },
    );
    state.read(cx).input.clone()
}
