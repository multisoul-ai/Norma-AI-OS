use super::BrowserAddressInput;
use gpui::{
    div, fill, point, prelude::*, px, relative, rgba, size, App, Bounds, Context, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, MouseButton, PaintQuad, Pixels, ShapedLine, Style, TextRun,
    UnderlineStyle, Window,
};

pub(super) struct BrowserAddressInputElement {
    input: Entity<BrowserAddressInput>,
}

pub(super) struct BrowserAddressInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for BrowserAddressInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BrowserAddressInputElement {
    type RequestLayoutState = ();
    type PrepaintState = BrowserAddressInputPrepaintState;

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
        let style = window.text_style();
        let base_run = TextRun {
            len: input.content.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = input
            .marked_range
            .as_ref()
            .map(|marked| {
                vec![
                    TextRun {
                        len: marked.start,
                        ..base_run.clone()
                    },
                    TextRun {
                        len: marked.end.saturating_sub(marked.start),
                        underline: Some(UnderlineStyle {
                            color: Some(base_run.color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..base_run.clone()
                    },
                    TextRun {
                        len: input.content.len().saturating_sub(marked.end),
                        ..base_run.clone()
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            })
            .unwrap_or_else(|| vec![base_run]);
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line =
            window
                .text_system()
                .shape_line(input.content.clone().into(), font_size, &runs, None);
        let cursor_x = line.x_for_index(input.cursor());
        let (selection, cursor) = if input.selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top()),
                        size(px(1.5), bounds.size.height),
                    ),
                    rgba((input.theme.accent_cyan & 0xffffff00) | 0xd8),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(input.selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(input.selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba((input.theme.accent_cyan & 0xffffff00) | 0x35),
                )),
                None,
            )
        };
        BrowserAddressInputPrepaintState {
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
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl gpui::Render for BrowserAddressInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("browser-address-input")
            .key_context("BrowserAddressInput")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(BrowserAddressInput::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(BrowserAddressInput::on_mouse_down),
            )
            .child(BrowserAddressInputElement { input: cx.entity() })
    }
}
