use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, FocusHandle,
    KeyDownEvent, MouseDownEvent, Pixels, ShapedLine, SharedString, Subscription, Task, WeakEntity,
    Window,
};
use std::ops::Range;
use ui::Theme;

pub(crate) struct VoiceSettingsInputs {
    base_url: Entity<VoiceTextInput>,
    model: Entity<VoiceTextInput>,
    api_key: Entity<VoiceTextInput>,
}

impl VoiceSettingsInputs {
    pub(crate) fn load_default(
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        theme: Theme,
        cx: &mut Context<MultisoulApp>,
    ) -> (VoiceSettingsState, Self) {
        let state = VoiceSettingsState::load_default();
        let inputs = Self::new(app, root_focus, &state, theme, cx);
        (state, inputs)
    }

    pub(crate) fn new(
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        state: &VoiceSettingsState,
        theme: Theme,
        cx: &mut Context<MultisoulApp>,
    ) -> Self {
        let base_url = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::BaseUrl,
                value: state.editor_value(VoiceSettingsField::BaseUrl),
                placeholder: "https://api.example.com/v1",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let model = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::Model,
                value: state.editor_value(VoiceSettingsField::Model),
                placeholder: "Model name",
                masked: false,
                theme,
            },
            app.clone(),
            root_focus.clone(),
            cx,
        );
        let api_key = new_input(
            VoiceInputConfig {
                field: VoiceSettingsField::ApiKey,
                value: state.editor_value(VoiceSettingsField::ApiKey),
                placeholder: status::input_placeholder(state, VoiceSettingsField::ApiKey),
                masked: true,
                theme,
            },
            app,
            root_focus,
            cx,
        );
        Self {
            base_url,
            model,
            api_key,
        }
    }

    pub(crate) fn input(&self, field: VoiceSettingsField) -> Entity<VoiceTextInput> {
        match field {
            VoiceSettingsField::BaseUrl => self.base_url.clone(),
            VoiceSettingsField::Model => self.model.clone(),
            VoiceSettingsField::ApiKey => self.api_key.clone(),
        }
    }

    pub(crate) fn sync_from_state(
        &self,
        state: &VoiceSettingsState,
        cx: &mut Context<MultisoulApp>,
    ) {
        for field in [
            VoiceSettingsField::BaseUrl,
            VoiceSettingsField::Model,
            VoiceSettingsField::ApiKey,
        ] {
            self.sync_field_from_state(field, state, cx);
        }
    }

    pub(crate) fn set_theme(&self, theme: Theme, cx: &mut Context<MultisoulApp>) {
        self.base_url
            .update(cx, |input, cx| input.set_theme(theme, cx));
        self.model
            .update(cx, |input, cx| input.set_theme(theme, cx));
        self.api_key
            .update(cx, |input, cx| input.set_theme(theme, cx));
    }

    pub(crate) fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.base_url.read(cx).focus_handle.is_focused(window)
            || self.model.read(cx).focus_handle.is_focused(window)
            || self.api_key.read(cx).focus_handle.is_focused(window)
    }

    pub(crate) fn input_surface_size(
        &self,
        field: VoiceSettingsField,
        cx: &App,
    ) -> Option<(u32, u32)> {
        self.input(field).read(cx).last_bounds.map(|bounds| {
            let width = (bounds.size.width / gpui::px(1.0)).max(0.0).round() as u32;
            let height = (bounds.size.height / gpui::px(1.0)).max(0.0).round() as u32;
            (width, height)
        })
    }

    pub(crate) fn rendered_text(&self, field: VoiceSettingsField, cx: &App) -> Option<String> {
        self.input(field)
            .read(cx)
            .last_layout
            .as_ref()
            .map(|layout| layout.text.to_string())
    }
}

struct VoiceInputConfig {
    field: VoiceSettingsField,
    value: String,
    placeholder: &'static str,
    masked: bool,
    theme: Theme,
}

fn new_input(
    config: VoiceInputConfig,
    app: WeakEntity<MultisoulApp>,
    root_focus: FocusHandle,
    cx: &mut Context<MultisoulApp>,
) -> Entity<VoiceTextInput> {
    cx.new(|cx| VoiceTextInput::new(config, app, root_focus, cx))
}

pub(crate) struct VoiceTextInput {
    field: VoiceSettingsField,
    content: SharedString,
    original_content: SharedString,
    placeholder: SharedString,
    masked: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    focus_handle: FocusHandle,
    root_focus: FocusHandle,
    app: WeakEntity<MultisoulApp>,
    theme: Theme,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    auto_save_task: Option<Task<()>>,
    focus_subscription: Option<Subscription>,
}

impl VoiceTextInput {
    fn new(
        config: VoiceInputConfig,
        app: WeakEntity<MultisoulApp>,
        root_focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let cursor = config.value.len();
        let content: SharedString = config.value.into();
        Self {
            field: config.field,
            original_content: content.clone(),
            content,
            placeholder: config.placeholder.into(),
            masked: config.masked,
            selected_range: cursor..cursor,
            selection_reversed: false,
            marked_range: None,
            focus_handle: cx.focus_handle().tab_stop(true),
            root_focus,
            app,
            theme: config.theme,
            last_layout: None,
            last_bounds: None,
            auto_save_task: None,
            focus_subscription: None,
        }
    }

    fn sync_value(&mut self, value: String, cx: &mut Context<Self>) {
        let cursor = value.len();
        self.content = value.into();
        self.original_content = self.content.clone();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.last_bounds = None;
        self.auto_save_task = None;
        cx.notify();
    }

    fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let tab_navigation =
            !modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let handled = match key {
            "enter" | "escape" if self.marked_range.is_some() => return,
            "tab" if tab_navigation && modifiers.shift => {
                window.focus_prev();
                true
            }
            "tab" if tab_navigation => {
                window.focus_next();
                true
            }
            "enter" => self.submit(window, cx),
            "escape" => self.cancel(window, cx),
            "backspace" => self.backspace(window, cx),
            "delete" => self.delete(window, cx),
            "left" if modifiers.shift => {
                self.select_to(self.previous_boundary(self.cursor_offset()), cx)
            }
            "right" if modifiers.shift => {
                self.select_to(self.next_boundary(self.cursor_offset()), cx)
            }
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
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx)
    }

    fn right(&mut self, cx: &mut Context<Self>) -> bool {
        let offset = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx)
    }

    fn backspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
        true
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
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

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.masked && self.content.is_empty() {
            self.finish_focus_session(cx);
            self.root_focus.focus(window);
            return true;
        }
        let value = self.content.to_string();
        let saved = self.app.upgrade().and_then(|app| {
            app.update(cx, |app, cx| {
                app.commit_voice_text_input(self.field, value, cx)
            })
        });
        if let Some(saved) = saved {
            if self.masked {
                self.sync_value_with_placeholder(
                    String::new(),
                    status::CONFIGURED_API_KEY_PLACEHOLDER,
                    cx,
                );
            } else {
                self.sync_value(saved, cx);
            }
            self.root_focus.focus(window);
        }
        cx.notify();
        true
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.finish_focus_session(cx);
        self.root_focus.focus(window);
        true
    }

    fn update_parent_draft(&self, begin: bool, cx: &mut Context<Self>) {
        let value = self.content.to_string();
        if let Some(app) = self.app.upgrade() {
            app.update(cx, |app, cx| {
                if begin {
                    app.begin_voice_text_input(self.field, value, cx);
                } else {
                    app.update_voice_text_input(self.field, value, cx);
                }
            });
        }
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

    fn cursor_offset(&self) -> usize {
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
        let mut utf8 = 0;
        let mut utf16 = 0;
        for character in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += character.len_utf16();
            utf8 += character.len_utf8();
        }
        utf8
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

mod autosave;
mod focus;
mod native;
mod status;

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

#[cfg(test)]
#[path = "../../../../tests/unit/voice_text_input_tests.rs"]
mod voice_text_input_tests;
