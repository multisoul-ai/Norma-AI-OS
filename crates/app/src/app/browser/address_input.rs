//! Browser 地址栏的原生 GPUI 文本输入。
//!
//! 不使用 `gpui-component::Input`：该组件要求窗口根为 `component::Root`，而本应用的
//! 根实体是 `MultisoulApp`。这里直接实现 `EntityInputHandler`，把 macOS IME、系统文本
//! 服务和标准编辑命令接到浏览器地址栏。

use gpui::{
    point, App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, KeyDownEvent, MouseDownEvent, Pixels, Point, ShapedLine, UTF16Selection, Window,
};
use std::ops::Range;
use ui::Theme;

mod render;

#[derive(Clone, Copy, Debug)]
pub(crate) enum BrowserAddressInputEvent {
    Submit,
    Cancel,
}

pub(crate) struct BrowserAddressInput {
    content: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    focus_handle: FocusHandle,
    theme: Theme,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl EventEmitter<BrowserAddressInputEvent> for BrowserAddressInput {}

impl BrowserAddressInput {
    pub(crate) fn new(value: String, theme: Theme, cx: &mut Context<Self>) -> Self {
        let length = value.len();
        Self {
            content: value,
            selected_range: 0..length,
            selection_reversed: false,
            marked_range: None,
            focus_handle: cx.focus_handle().tab_stop(true),
            theme,
            last_layout: None,
            last_bounds: None,
        }
    }

    pub(crate) fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    pub(crate) fn value(&self) -> String {
        self.content.clone()
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
            self.select_all(cx);
        }
        cx.stop_propagation();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let handled = match key {
            "enter" | "escape" if self.marked_range.is_some() => return,
            "enter" => {
                cx.emit(BrowserAddressInputEvent::Submit);
                true
            }
            "escape" => {
                cx.emit(BrowserAddressInputEvent::Cancel);
                true
            }
            "backspace" => self.backspace(window, cx),
            "delete" => self.delete(window, cx),
            "left" if modifiers.shift => self.select_to(self.previous_boundary(self.cursor()), cx),
            "right" if modifiers.shift => self.select_to(self.next_boundary(self.cursor()), cx),
            "left" => self.left(cx),
            "right" => self.right(cx),
            "home" => self.move_to(0, cx),
            "end" => self.move_to(self.content.len(), cx),
            _ if platform_only && key.eq_ignore_ascii_case("a") => self.select_all(cx),
            _ if platform_only && key.eq_ignore_ascii_case("v") => self.paste(window, cx),
            _ if platform_only && key.eq_ignore_ascii_case("c") => self.copy(cx),
            _ if platform_only && key.eq_ignore_ascii_case("x") => self.cut(window, cx),
            _ => return,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn left(&mut self, cx: &mut Context<Self>) -> bool {
        let offset = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx)
    }

    fn right(&mut self, cx: &mut Context<Self>) -> bool {
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx)
    }

    fn backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
        true
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
        true
    }

    fn select_all(&mut self, cx: &mut Context<Self>) -> bool {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
        true
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &single_line(&text), window, cx);
        }
        true
    }

    fn copy(&self, cx: &mut Context<Self>) -> bool {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
        true
    }

    fn cut(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.copy(cx);
        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
        true
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
        true
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
        true
    }

    fn cursor(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .char_indices()
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .char_indices()
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf8_offset_from_utf16(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

impl EntityInputHandler for BrowserAddressInput {
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
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let new_text = single_line(new_text);
        self.replace_range(range, &new_text, None, false, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        selection_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let new_text = single_line(new_text);
        let selection = selection_utf16.map(|selection| {
            let start = utf8_offset_from_utf16(&new_text, selection.start);
            let end = utf8_offset_from_utf16(&new_text, selection.end);
            range.start + start..range.start + end
        });
        self.replace_range(range, &new_text, selection, true, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
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
        let offset = layout.closest_index_for_x(point.x - bounds.left());
        Some(self.offset_to_utf16(utf8_boundary(&self.content, offset)))
    }
}

impl BrowserAddressInput {
    fn replace_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        selection: Option<Range<usize>>,
        marked: bool,
        cx: &mut Context<Self>,
    ) {
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        );
        self.marked_range = marked
            .then_some(range.start..range.start + new_text.len())
            .filter(|range| !range.is_empty());
        self.selected_range = selection.unwrap_or_else(|| {
            let cursor = range.start + new_text.len();
            cursor..cursor
        });
        self.selection_reversed = false;
        cx.notify();
    }
}

impl Focusable for BrowserAddressInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn single_line(text: &str) -> String {
    text.chars()
        .filter(|character| !matches!(character, '\n' | '\r'))
        .collect()
}

fn utf8_offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for character in text.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += character.len_utf16();
        utf8 += character.len_utf8();
    }
    utf8
}

fn utf8_boundary(text: &str, offset: usize) -> usize {
    let mut boundary = offset.min(text.len());
    while !text.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    boundary
}
