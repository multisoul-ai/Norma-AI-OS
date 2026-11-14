use super::super::*;
use gpui::rgba;
use norma::protocol::{QuestionOption, StaffQuestion};
use ui::component::{checkbox::Checkbox, radio::Radio, Disableable};

pub(super) fn choice(
    ctx: &BlockCtx,
    question: &StaffQuestion,
    index: usize,
    option: Option<(usize, &QuestionOption)>,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let item = &question.items[index];
    let theme = ctx.app.staff_theme();
    let available = question
        .worker
        .as_ref()
        .is_none_or(|target| ctx.app.runtime.native_question_available(target));
    let qid = question.id.clone();
    let key = option.map(|(_, option)| option.key.clone());
    let selector = format!(
        "staff-question-option-{}-{index}-{}",
        question.id,
        option.map_or_else(|| "custom".into(), |(index, _)| index.to_string())
    );
    let label = option.map_or_else(
        || ui::i18n::text("staff.question_custom").to_owned(),
        |(_, option)| option.label.clone(),
    );
    let content = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_4))
        .child(label)
        .when_some(
            option.and_then(|(_, option)| option.description.as_ref()),
            |body, description| {
                body.child(
                    div()
                        .text_size(px(tokens::META_TEXT))
                        .text_color(rgba(theme.text_secondary))
                        .child(description.clone()),
                )
            },
        );
    let click = cx.listener(move |app, _: &bool, _, cx| {
        if let Some(editor) = app.staff.question_editors.get_mut(&qid) {
            if let Some(key) = &key {
                editor.choose(index, key);
            } else {
                editor.toggle_custom(index);
            }
        }
        app.notify_staff(cx);
    });
    let bg = rgba(if selected {
        ui::icons::with_alpha(theme.accent_cyan, 25)
    } else {
        theme.panel
    });
    let border = rgba(if selected {
        theme.accent_cyan
    } else {
        ctx.app.staff_line()
    });
    let hover = rgba(ui::icons::with_alpha(theme.text_primary, 24));
    let pressed = rgba(ui::icons::with_alpha(theme.accent_cyan, 60));
    if item.multiple {
        Checkbox::new(gpui::SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .checked(selected)
            .disabled(ctx.app.staff.busy || !available)
            .w_full()
            .p(px(tokens::SPACE_12))
            .rounded(px(tokens::SPACE_12))
            .border_1()
            .border_color(border)
            .bg(bg)
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(pressed))
            .child(content)
            .on_click(click)
            .into_any_element()
    } else {
        Radio::new(gpui::SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .checked(selected)
            .disabled(ctx.app.staff.busy || !available)
            .w_full()
            .p(px(tokens::SPACE_12))
            .rounded(px(tokens::SPACE_12))
            .border_1()
            .border_color(border)
            .bg(bg)
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(pressed))
            .child(content)
            .on_click(click)
            .into_any_element()
    }
}
