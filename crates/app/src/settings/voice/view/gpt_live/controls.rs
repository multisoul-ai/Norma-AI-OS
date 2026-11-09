use super::*;
use ui::component::button::ButtonRounded;

/// Material segmented control: one pill, the selected segment filled and checked.
pub(super) fn segmented(
    theme: Theme,
    options: [(VoiceInteractionMode, &'static str, &'static str); 2],
    selected: VoiceInteractionMode,
    cx: &Context<MultisoulApp>,
    select: impl Fn(&mut MultisoulApp, VoiceInteractionMode, &mut Context<MultisoulApp>)
        + Clone
        + 'static,
) -> Div {
    div()
        .flex()
        .flex_none()
        .p(px(2.0))
        .rounded_full()
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x30)))
        .children(options.map(|(value, selector, label)| {
            let active = value == selected;
            let select = select.clone();
            div()
                .id(selector)
                .debug_selector(move || selector.into())
                .child(
                    Button::new(selector)
                        .custom(choice_variant(theme, active, cx))
                        .rounded(ButtonRounded::Size(px(999.0)))
                        .compact()
                        .child(
                            div()
                                .min_w(px(72.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(px(5.0))
                                .settings_font(TextRole::Control)
                                .text_color(rgba(if active {
                                    theme.accent_blue
                                } else {
                                    theme.text_primary
                                }))
                                .when(active, |label| {
                                    label.child(icon("icons/check.svg", theme.accent_blue, 13.0))
                                })
                                .child(ui::i18n::text(label)),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| select(app, value, cx))),
                )
        }))
}

/// Outlined radio chip for the reply voice.
pub(super) fn voice_option(
    theme: Theme,
    id: &'static str,
    selector: &'static str,
    label: &'static str,
    selected: bool,
    cx: &Context<MultisoulApp>,
) -> gpui::Stateful<Div> {
    let ring = if selected {
        theme.accent_blue
    } else {
        with_alpha(theme.text_primary, 0x50)
    };
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .child(
            Button::new(selector)
                .custom(
                    choice_variant(theme, selected, cx).border(
                        rgba(if selected {
                            theme.accent_blue
                        } else {
                            with_alpha(theme.text_primary, 0x30)
                        })
                        .into(),
                    ),
                )
                .rounded(ButtonRounded::Size(px(10.0)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .settings_font(TextRole::Control)
                        .text_color(rgba(if selected {
                            theme.accent_blue
                        } else {
                            theme.text_primary
                        }))
                        .child(
                            div()
                                .size(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border(px(1.5))
                                .border_color(rgba(ring))
                                .when(selected, |dot| {
                                    dot.child(div().size(px(6.0)).rounded_full().bg(rgba(ring)))
                                }),
                        )
                        .child(label),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.voice_settings.select_gpt_live_voice(id);
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                })),
        )
}

fn choice_variant(theme: Theme, selected: bool, cx: &Context<MultisoulApp>) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(
            rgba(if selected {
                with_alpha(theme.accent_blue, 0x24)
            } else {
                with_alpha(theme.accent_blue, 0x00)
            })
            .into(),
        )
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(with_alpha(theme.accent_blue, 0x00)).into())
        .hover(
            rgba(with_alpha(
                theme.accent_blue,
                if selected { 0x34 } else { 0x14 },
            ))
            .into(),
        )
        .active(rgba(with_alpha(theme.accent_blue, 0x44)).into())
}
