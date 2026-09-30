use super::{action_button, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, uniform_list, Context, Div, FontWeight, IntoElement};
use std::ops::Range;
use ui::Theme;

const VOICE_ROW_HEIGHT: f32 = 64.0;
const MAX_VISIBLE_ROWS: usize = 5;

#[derive(Clone)]
struct VoiceListRow {
    voice: tts::ProviderVoiceDescriptor,
    origin_position: usize,
}

pub(super) fn minimax_voice_catalog(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let mut origin_counts = [0usize; 3];
    let voices = state
        .filtered_minimax_voices()
        .into_iter()
        .map(|voice| {
            let origin = origin_index(voice.origin());
            let origin_position = origin_counts[origin];
            origin_counts[origin] += 1;
            VoiceListRow {
                voice: voice.clone(),
                origin_position,
            }
        })
        .collect::<Vec<_>>();
    let selected = state.tts_picker_minimax_voice_id().cloned();
    let list_height = voices.len().clamp(1, MAX_VISIBLE_ROWS) as f32 * VOICE_ROW_HEIGHT;
    let item_count = voices.len();

    div()
        .id("settings-tts-minimax-voice-list")
        .debug_selector(|| "settings-tts-minimax-voice-list".to_string())
        .w_full()
        .h(px(list_height))
        .flex()
        .flex_col()
        .rounded(px(9.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x42)))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            uniform_list(
                "settings-tts-minimax-voice-list-items",
                item_count,
                cx.processor(move |_app, range: Range<usize>, _window, cx| {
                    range
                        .filter_map(|index| voices.get(index))
                        .map(|row| voice_row(theme, row, selected.as_ref(), cx))
                        .collect::<Vec<_>>()
                }),
            )
            .h_full()
            .w_full(),
        )
}

fn voice_row(
    theme: Theme,
    row: &VoiceListRow,
    selected: Option<&tts::TtsVoiceId>,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let voice_id = row.voice.id().clone();
    let preview_voice_id = voice_id.clone();
    let origin = row.voice.origin();
    let origin_position = row.origin_position;
    let stable_index = origin_index(origin) * 10_000 + origin_position;
    let selected_here = selected == Some(&voice_id);
    let display_name = row.voice.display_name().to_string();
    let voice_id_label = voice_id.as_str().to_string();
    div().h(px(VOICE_ROW_HEIGHT)).w_full().p(px(4.0)).child(
        div()
            .id(("settings-tts-minimax-voice", stable_index))
            .debug_selector(move || {
                format!(
                    "settings-tts-minimax-voice-{}-{origin_position}",
                    origin_index(origin)
                )
            })
            .size_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgba(with_alpha(
                if selected_here {
                    theme.accent_blue
                } else {
                    theme.edge_highlight
                },
                if selected_here { 0x72 } else { 0x32 },
            )))
            .cursor_pointer()
            .child(
                action_button(theme, "▶", theme.accent_blue)
                    .id(("settings-tts-minimax-preview-voice", stable_index))
                    .debug_selector(move || {
                        format!(
                            "settings-tts-minimax-preview-voice-{}-{origin_position}",
                            origin_index(origin)
                        )
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.preview_minimax_catalog_voice(preview_voice_id.clone(), cx)
                    })),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(theme.text_primary))
                            .child(display_name),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child(voice_id_label),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(theme.text_tertiary))
                            .child(origin_label(origin)),
                    )
                    .when(selected_here, |status| {
                        status.child(
                            div()
                                .text_size(px(9.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.accent_blue))
                                .child("DRAFT"),
                        )
                    }),
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                app.on_voice_select_minimax_catalog_voice(voice_id.clone(), cx)
            })),
    )
}

fn origin_label(origin: tts::TtsVoiceOrigin) -> &'static str {
    match origin {
        tts::TtsVoiceOrigin::System => "SYSTEM",
        tts::TtsVoiceOrigin::Cloned => "CLONED",
        tts::TtsVoiceOrigin::Generated => "GENERATED",
    }
}

fn origin_index(origin: tts::TtsVoiceOrigin) -> usize {
    match origin {
        tts::TtsVoiceOrigin::System => 0,
        tts::TtsVoiceOrigin::Cloned => 1,
        tts::TtsVoiceOrigin::Generated => 2,
    }
}
