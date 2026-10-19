//! Conversation message appearance shared by voice and typed turns.

use super::{GLASS_TEXT, GLASS_TEXT_SECONDARY};
use crate::app::assistant_conversation::Speaker;
use gpui::{div, prelude::*, px, rgba, Div, ElementId, Stateful};
use ui::component::StyledExt;

pub(super) fn conversation_message(
    index: usize,
    speaker: Speaker,
    text: String,
    first: bool,
    width: f32,
) -> Stateful<Div> {
    let is_user = speaker == Speaker::User;
    let id = format!("desktop-assistant-message-{index}");
    div()
        .id(ElementId::Name(id.clone().into()))
        .debug_selector(move || id.clone())
        .max_w(px((width - 50.0) * 0.92))
        .when(is_user, |message| {
            message
                .ml_auto()
                .px(px(14.0))
                .py(px(10.0))
                .rounded(px(16.0))
                .border_1()
                .border_color(rgba(0xffffff19))
                .bg(rgba(0xffffff0c))
        })
        .when(!is_user && !first, |message| {
            message
                .pl(px(12.0))
                .border_l_2()
                .border_color(rgba(0xb8ceea55))
        })
        .text_size(px(12.0))
        .line_height(px(20.0))
        .text_color(rgba(GLASS_TEXT))
        .when(!is_user, |message| {
            message.child(
                div()
                    .mb(px(6.0))
                    .font_semibold()
                    .text_size(px(10.0))
                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                    .child("Norma"),
            )
        })
        .child(div().child(text))
}
