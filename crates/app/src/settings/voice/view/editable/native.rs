use super::{single_line, utf8_offset_from_utf16, VoiceTextInput};
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{
    div, fill, point, prelude::*, px, relative, rgba, size, App, Bounds, Context, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, PaintQuad, Pixels,
    Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use std::ops::Range;

impl VoiceTextInput {
    fn display_text(&self) -> SharedString {
        if self.content.is_empty() {
            self.placeholder.clone()
        } else if self.masked && !self.revealed {
            "•".repeat(self.content.chars().count()).into()
        } else {
            self.content.clone()
        }
    }

    fn display_offset(&self, content_offset: usize) -> usize {
        if !self.masked {
            return content_offset.min(self.content.len());
        }
        self.content[..content_offset]
            .chars()
            .count()
            .saturating_mul('•'.len_utf8())
    }

    fn display_range(&self, content_range: &Range<usize>) -> Range<usize> {
        self.display_offset(content_range.start)..self.display_offset(content_range.end)
    }

    fn content_offset_for_display(&self, display_offset: usize) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        if self.masked {
            let character_count = display_offset / '•'.len_utf8();
            return self
                .content
                .char_indices()
                .nth(character_count)
                .map(|(offset, _)| offset)
                .unwrap_or(self.content.len());
        }
        let mut offset = display_offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        offset
    }
}

impl EntityInputHandler for VoiceTextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let had_marked_text = self.marked_range.take().is_some();
        if had_marked_text {
            self.auto_save_parent(cx);
        }
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_content = self.content.clone();
        let had_marked_text = self.marked_range.is_some();
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let new_text = single_line(new_text);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        if had_marked_text || self.content != previous_content {
            self.update_parent_draft(false, cx);
            self.auto_save_parent(cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        selection_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_content = self.content.clone();
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let new_text = single_line(new_text);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = selection_utf16
            .as_ref()
            .map(|selection| {
                utf8_offset_from_utf16(&new_text, selection.start)
                    ..utf8_offset_from_utf16(&new_text, selection.end)
            })
            .map(|selection| range.start + selection.start..range.start + selection.end)
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });
        self.selection_reversed = false;
        if self.content != previous_content {
            self.update_parent_draft(false, cx);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.display_range(&self.range_from_utf16(&range_utf16));
        Some(Bounds::from_corners(
            point(
                bounds.left() + layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let display_offset = layout.closest_index_for_x(point.x - bounds.left());
        Some(self.offset_to_utf16(self.content_offset_for_display(display_offset)))
    }
}

struct VoiceInputElement {
    input: Entity<VoiceTextInput>,
}

struct VoiceInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for VoiceInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for VoiceInputElement {
    type RequestLayoutState = ();
    type PrepaintState = VoiceInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let display_text = input.display_text();
        let selected_range = input.display_range(&input.selected_range);
        let cursor = input.display_offset(input.cursor_offset());
        let marked_range = input
            .marked_range
            .as_ref()
            .map(|range| input.display_range(range));
        let accent = input.theme.accent_orange;
        let style = window.text_style();
        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = marked_range {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..base_run.clone()
                },
                TextRun {
                    len: marked_range.end.saturating_sub(marked_range.start),
                    underline: Some(UnderlineStyle {
                        color: Some(base_run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..base_run.clone()
                },
                TextRun {
                    len: display_text.len().saturating_sub(marked_range.end),
                    ..base_run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![base_run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_x = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top()),
                        size(px(1.5), bounds.size.height),
                    ),
                    rgba((accent & 0xffffff00) | 0xd8),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba((accent & 0xffffff00) | 0x35),
                )),
                None,
            )
        };
        VoiceInputPrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if focus.is_focused(window) {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
        }
        let Some(line) = prepaint.line.take() else {
            return;
        };
        let _ = line.paint(bounds.origin, window.line_height(), window, cx);
        if focus.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for VoiceTextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_focus_subscriptions(window, cx);
        let focused = self.focus_handle.is_focused(window);
        div()
            .id(("settings-voice-field", self.field.index()))
            .debug_selector({
                let field = self.field;
                move || {
                    format!(
                        "settings-voice-field-{}",
                        field.stable_name().replace('_', "-")
                    )
                }
            })
            .key_context("VoiceTextInput")
            .track_focus(&self.focus_handle)
            .w_full()
            .max_w(px(420.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgba(
                (if focused {
                    self.theme.accent_orange
                } else {
                    self.theme.edge_highlight
                } & 0xffffff00)
                    | if focused { 0x90 } else { 0x60 },
            ))
            .bg(rgba((self.theme.panel_high & 0xffffff00) | 0xb8))
            .settings_font(TextRole::Body)
            .text_color(rgba(if self.content.is_empty() {
                self.theme.text_tertiary
            } else {
                self.theme.text_secondary
            }))
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .child(VoiceInputElement { input: cx.entity() })
    }
}

impl Focusable for VoiceTextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
