//! Diff main view: summary pills, file header and a line-numbered, semantically colored body.

use super::diff::{diff_stats, is_binary_patch, parse_diff_for_display, DiffKind, DiffLine};
use super::document::{
    display_line, numbered_line, GREEN, GUTTER, LINE, PATH_BAR, RED, TEXT, TEXT_MUTED,
};
use super::*;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, uniform_list, AnyElement, Context, Div, ElementId,
    ListHorizontalSizingBehavior,
};

const ADD_BG: u32 = 0x12281dff;
const REMOVE_BG: u32 = 0x2b1618ff;

impl MultisoulApp {
    pub(super) fn render_workbench_diff(&self, cx: &mut Context<Self>) -> AnyElement {
        let s = &self.desktop_workspace.workbench;
        let patch = s.diff.as_deref().unwrap_or_default();
        let lines = s
            .diff_parsed
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(parse_diff_for_display(patch)));
        let (added, removed) = diff_stats(&lines);
        let local = s.saved.comparison == Comparison::Local;
        let scope = match (&s.saved.comparison, s.diff_staged) {
            (Comparison::Local, true) => ui::i18n::text("drawers.staged").to_string(),
            (Comparison::Local, false) => ui::i18n::text("drawers.unstaged").to_string(),
            (Comparison::Branch(target), _) => format!(
                "{} → {target}",
                s.git
                    .as_ref()
                    .and_then(|g| g.branch.as_deref())
                    .unwrap_or_else(|| ui::i18n::text("drawers.content_no_commits"))
            ),
        };
        let pill = |children: Vec<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(32.0))
                .px(px(13.0))
                .rounded(px(16.0))
                .bg(rgba(0xffffff10))
                .text_size(px(12.0))
                .text_color(rgba(0xe4e9f1ff))
                .children(children)
        };
        let counts = |bold: bool| {
            vec![
                div()
                    .text_color(rgba(GREEN))
                    .font_weight(if bold {
                        gpui::FontWeight::SEMIBOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(format!("+{added}"))
                    .into_any_element(),
                div()
                    .text_color(rgba(RED))
                    .child(format!("-{removed}"))
                    .into_any_element(),
            ]
        };
        let mut summary = vec![pill(vec![div().child(scope).into_any_element()])];
        if !local {
            let mut with_counts = vec![div()
                .child(ui::i18n::text("drawers.branch"))
                .into_any_element()];
            with_counts.extend(counts(true));
            summary.insert(0, pill(with_counts));
        } else {
            let mut with_counts = vec![div()
                .child(ui::i18n::text("drawers.changes"))
                .into_any_element()];
            with_counts.extend(counts(true));
            summary.push(pill(with_counts));
        }
        let head = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(9.0))
            .bg(rgba(PATH_BAR))
            .border_y_1()
            .border_color(rgba(LINE))
            .text_size(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(s.saved.selected.clone().unwrap_or_default()),
            )
            .child(div().text_color(rgba(GREEN)).child(format!("+{added}")))
            .child(div().text_color(rgba(RED)).child(format!("-{removed}")));
        let body: AnyElement = if is_binary_patch(patch) || lines.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(rgba(TEXT_MUTED))
                .child(if is_binary_patch(patch) {
                    ui::i18n::text("drawers.binary_diff")
                } else {
                    ui::i18n::text("drawers.no_diff")
                })
                .into_any_element()
        } else {
            let count = lines.len();
            let longest = lines
                .iter()
                .enumerate()
                .max_by_key(|(_, l)| super::text_index::display_width(&l.text))
                .map(|(i, _)| i);
            uniform_list(
                "files-diff-lines",
                count,
                cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                    range.map(|i| diff_row(&lines[i])).collect::<Vec<_>>()
                }),
            )
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .with_width_from_item(longest)
            .font_family("Menlo")
            .text_size(px(12.0))
            .flex_1()
            .min_h(px(0.0))
            .into_any_element()
        };
        div()
            .id(ElementId::Name("files-diff".into()))
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .children(summary),
            )
            .child(head)
            .child(body)
            .into_any_element()
    }
}

fn diff_row(line: &DiffLine) -> Div {
    let text = display_line(&line.text);
    let number = line.number.unwrap_or(0);
    let (gutter, color, bg) = match line.kind {
        DiffKind::Add => (0x7fa896ff, TEXT, Some(ADD_BG)),
        DiffKind::Remove => (0xb98084ff, TEXT, Some(REMOVE_BG)),
        DiffKind::Context => (GUTTER, TEXT, None),
        DiffKind::Hunk => (GUTTER, 0x8db9ecff, Some(0xffffff0a)),
        DiffKind::Meta => (GUTTER, TEXT_MUTED, None),
    };
    if line.number.is_none() {
        return numbered_line(0, 0x00000000, text, color, bg);
    }
    let marker = match line.kind {
        DiffKind::Add => "+ ",
        DiffKind::Remove => "- ",
        _ => "  ",
    };
    numbered_line(number, gutter, format!("{marker}{text}"), color, bg)
}
