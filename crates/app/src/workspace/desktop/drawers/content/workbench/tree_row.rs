//! Tree row primitives for the explorer sidebar: glyphs, chevrons and interactive rows.

use super::document::{tooltip_view, GREEN, TEXT, TEXT_MUTED, TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, svg, Context, Div, ElementId, KeyDownEvent, Stateful, Window,
};
use std::rc::Rc;

pub(super) const ROW_HEIGHT: f32 = 28.0;
pub(super) const HOVER: u32 = 0xffffff14;
pub(super) const SELECTED: u32 = 0xffffff1f;
pub(super) const PRESSED: u32 = 0xffffff2a;
pub(super) const FOCUS: u32 = 0x8db9ecaa;
pub(super) const INDENT: f32 = 12.0;

pub(super) type Action = Rc<dyn Fn(&mut MultisoulApp, &mut Window, &mut Context<MultisoulApp>)>;

pub(super) enum Glyph {
    Markdown,
    GitDot,
    CiDot,
    Plain,
}

pub(super) fn glyph_for(name: &str) -> Glyph {
    let lower = name.to_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Glyph::Markdown
    } else if lower == ".gitignore" || lower == ".gitattributes" {
        Glyph::GitDot
    } else if lower.ends_with("-ci.yml") || lower.ends_with("-ci.yaml") {
        Glyph::CiDot
    } else {
        Glyph::Plain
    }
}

pub(super) fn glyph_element(glyph: Glyph) -> gpui::AnyElement {
    match glyph {
        Glyph::Markdown => div()
            .w(px(12.0))
            .flex_none()
            .font_family("Menlo")
            .text_size(px(9.0))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(rgba(GREEN))
            .child(ui::i18n::text("drawers.external_modified_badge"))
            .into_any_element(),
        Glyph::GitDot => dot(false),
        Glyph::CiDot => dot(true),
        Glyph::Plain => svg()
            .path("icons/file-document.svg")
            .w(px(12.0))
            .h(px(12.0))
            .flex_none()
            .text_color(rgba(TEXT_MUTED))
            .into_any_element(),
    }
}

fn dot(hollow: bool) -> gpui::AnyElement {
    let base = div().w(px(12.0)).h(px(12.0)).flex_none().rounded(px(3.0));
    if hollow {
        base.border_2()
            .border_color(rgba(0xe8553bff))
            .into_any_element()
    } else {
        base.bg(rgba(0xe8553bff)).into_any_element()
    }
}

pub(super) fn chevron(open: bool) -> gpui::AnyElement {
    let icon = svg()
        .path("icons/chevron-down.svg")
        .w(px(11.0))
        .h(px(11.0))
        .flex_none()
        .text_color(rgba(TEXT_MUTED));
    if open {
        icon.into_any_element()
    } else {
        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
            -std::f32::consts::FRAC_PI_2,
        )))
        .into_any_element()
    }
}

pub(super) fn activates(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}

pub(super) fn name_label(text: &str, color: u32, tooltip: &str) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("files-name-{tooltip}").into()))
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_color(rgba(color))
        .tooltip(tooltip_view(tooltip.to_string()))
        .child(text.to_string())
}

impl MultisoulApp {
    /// One 28px left-aligned tree row with hover, pressed, selected, focus and keyboard states.
    /// `expand` carries (currently open, toggle) for rows that collapse and expand.
    pub(super) fn tree_row(
        &self,
        id: ElementId,
        depth: usize,
        selected: bool,
        expand: Option<(bool, Action)>,
        activate: Action,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let click = activate.clone();
        div()
            .id(id)
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(7.0))
            .pl(px(6.0 + depth.min(12) as f32 * INDENT))
            .pr(px(8.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(12.0))
            .text_color(rgba(TEXT_SECONDARY))
            .cursor_pointer()
            .tab_index(0)
            .bg(rgba(if selected { SELECTED } else { 0x00000000 }))
            .hover(|s| s.bg(rgba(HOVER)))
            .active(|s| s.bg(rgba(PRESSED)))
            .focus(|s| s.border_color(rgba(FOCUS)))
            .on_click(cx.listener(move |app, _, w, cx| click(app, w, cx)))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, w, cx| {
                match event.keystroke.key.as_str() {
                    "enter" | "space" => activate(app, w, cx),
                    "down" => w.focus_next(),
                    "up" => w.focus_prev(),
                    "right" => match &expand {
                        Some((false, toggle)) => toggle(app, w, cx),
                        _ => return,
                    },
                    "left" => match &expand {
                        Some((true, toggle)) => toggle(app, w, cx),
                        _ => return,
                    },
                    _ => return,
                }
                cx.stop_propagation();
            }))
    }

    pub(super) fn sidebar_icon_button(
        &self,
        id: &'static str,
        glyph: &'static str,
        tip: &'static str,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key_action = action.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .w(px(22.0))
            .h(px(22.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(13.0))
            .text_color(rgba(TEXT_MUTED))
            .cursor_pointer()
            .tab_index(0)
            .tooltip(tooltip_view(tip))
            .hover(|s| s.bg(rgba(HOVER)).text_color(rgba(TEXT)))
            .active(|s| s.bg(rgba(PRESSED)))
            .focus(|s| s.border_color(rgba(FOCUS)))
            .on_click(cx.listener(move |app, _, w, cx| {
                cx.stop_propagation();
                action(app, w, cx)
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, w, cx| {
                if activates(event) {
                    cx.stop_propagation();
                    key_action(app, w, cx);
                }
            }))
            .child(glyph)
    }
}
