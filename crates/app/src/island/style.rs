//! Visual values and hierarchy from the approved browser prototype, in logical points.
use gpui::{
    div, prelude::*, px, rgb, rgba, AnyView, App, Context, Div, FontWeight, SharedString, Window,
};
use runtime::island::bloub::Mood;
use runtime::island::roster::AvatarFace;
use std::cell::Cell;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Sizable,
};

pub(super) const FG: u32 = 0xf4f6f8;
pub(super) const MUTED: u32 = 0x8a8e93;
pub(super) const FAINT: u32 = 0x505357;
pub(super) const RUN: u32 = 0x5fcbe0;
pub(super) const ATTENTION: u32 = 0xffa94d;
pub(super) const OK: u32 = 0x5ad38a;
pub(super) const STAFF: u32 = 0xa99bff;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tone {
    Neutral,
    Primary,
    Danger,
    Ghost,
    /// Staff accent: the prototype's purple report pill.
    Accent,
}

pub(super) fn action(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    tone: Tone,
    cx: &gpui::App,
) -> Button {
    let (bg, fg, hover, active) = match tone {
        Tone::Neutral => (0x1d1d1d, FG, 0x303030, 0x404040),
        Tone::Primary => (FG, 0x0a0c0f, 0xffffff, 0xd8dde2),
        Tone::Danger => (0x1d1d1d, 0xff7b7b, 0x3b2424, 0x503030),
        Tone::Ghost => (0x000000, MUTED, 0x1d1d1d, 0x303030),
        Tone::Accent => (0xb7a8ff, 0x120d2b, 0xc9bdff, 0x9d8df0),
    };
    Button::new(id.into())
        .small()
        .h(px(30.))
        .px(px(12.))
        .rounded(px(9.))
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label.into()),
        )
        .custom(
            ButtonCustomVariant::new(cx)
                .color(if tone == Tone::Ghost {
                    rgba(0).into()
                } else {
                    rgb(bg).into()
                })
                .foreground(rgb(fg).into())
                .border(rgba(0x00000000).into())
                .hover(rgb(hover).into())
                .active(rgb(active).into()),
        )
}

pub(super) fn card() -> Div {
    div()
        .p(px(12.))
        .rounded(px(16.))
        .bg(rgb(0x101010))
        .flex()
        .flex_col()
        .gap(px(10.))
}

thread_local! {
    static ANIMATE: Cell<bool> = const { Cell::new(true) };
}

/// One AppKit read per island frame. Avatars must not ask again while they paint.
pub(super) fn set_animations(enabled: bool) {
    ANIMATE.with(|cell| cell.set(enabled));
}

pub(super) fn animations_enabled() -> bool {
    ANIMATE.with(|cell| cell.get())
}

pub(super) fn avatar(
    name: &str,
    provider: &str,
    size: f32,
    face: AvatarFace,
    animate: bool,
) -> Div {
    avatar_with_ping(name, provider, size, face, animate, false)
}

/// A waiting avatar that also sends out the prototype's expanding attention ring.
pub(super) fn avatar_pinging(
    name: &str,
    provider: &str,
    size: f32,
    face: AvatarFace,
    animate: bool,
) -> Div {
    avatar_with_ping(name, provider, size, face, animate, true)
}

fn avatar_with_ping(
    name: &str,
    provider: &str,
    size: f32,
    face: AvatarFace,
    animate: bool,
    ping: bool,
) -> Div {
    let color = match provider.to_ascii_lowercase().as_str() {
        "claude" => 0xe08a66,
        "codex" => 0x6cc4b4,
        "cursor" => 0x93a6ff,
        "opencode" => 0xe6bb5c,
        "kimi" => 0xc29cff,
        _ => [0xe08a66, 0x6cc4b4, 0x93a6ff, 0xe6bb5c, 0xc29cff][name
            .bytes()
            .fold(0usize, |sum, b| sum.wrapping_add(b as usize))
            % 5],
    };
    super::bloub_view::bloub(name, color, size, face.mood, face.opacity, animate, ping)
}

pub(super) fn chip(label: impl Into<SharedString>, color: u32) -> Div {
    div()
        .flex_none()
        .h(px(19.))
        .px(px(7.))
        .rounded(px(6.))
        .bg(rgba((color << 8) | 0x21))
        .text_color(rgb(color))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .flex()
        .items_center()
        .child(label.into())
}

pub(super) fn card_heading(
    name: &str,
    provider: &str,
    context: String,
    title: &str,
    badge: &str,
    color: u32,
    mood: Mood,
) -> Div {
    card_heading_with(
        name,
        provider,
        context,
        title,
        chip(badge.to_owned(), color),
        mood,
    )
}

/// `trailing` is the right-hand end of the heading, e.g. a badge preceded by an action.
pub(super) fn card_heading_with(
    name: &str,
    provider: &str,
    context: String,
    title: &str,
    trailing: impl IntoElement,
    mood: Mood,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .child(avatar(
            name,
            provider,
            40.,
            AvatarFace { mood, opacity: 1. },
            animations_enabled(),
        ))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .flex()
                        .gap(px(5.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.to_owned()),
                        )
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(FAINT))
                        .truncate()
                        .child(context),
                ),
        )
        .child(trailing)
}

pub(super) fn empty(title: &'static str, detail: &'static str) -> Div {
    div()
        .w_full()
        .pt(px(28.))
        .pb(px(22.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .size(px(34.))
                .rounded(px(17.))
                .bg(rgba(0x5ad38a24))
                .text_color(rgb(OK))
                .flex()
                .items_center()
                .justify_center()
                .child(ui::component::Icon::new(ui::component::IconName::Check).size(px(16.))),
        )
        .child(div().text_size(px(13.)).child(ui::i18n::text(title)))
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(ui::i18n::text(detail)),
        )
}

/// Tooltip with fixed point sizes: the library tooltip scales with the window's rem size and
/// grew large enough to cover the neighbouring tab inside the island.
struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(rgb(0x1d1d1d))
            .border_1()
            .border_color(rgb(0x303030))
            .text_size(px(11.))
            .text_color(rgb(FG))
            .child(self.0.clone())
    }
}

pub(super) fn tip(label: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
    let label = label.into();
    move |_, cx| cx.new(|_| Tip(label.clone())).into()
}
