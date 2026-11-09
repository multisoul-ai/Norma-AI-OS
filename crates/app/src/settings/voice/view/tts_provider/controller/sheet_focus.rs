use crate::app::MultisoulApp;
use gpui::{Context, FocusHandle, Window};

impl MultisoulApp {
    pub(crate) fn on_voice_sheet_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.dismiss_tts_picker(window, cx);
            cx.stop_propagation();
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let tab = event.keystroke.key == "tab"
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.function;
        if tab {
            cycle_tts_sheet_focus(
                &self.voice_settings_inputs.tts_sheet_focus,
                window,
                cx,
                modifiers.shift,
            );
            cx.stop_propagation();
        }
    }
}

fn cycle_tts_sheet_focus(
    sheet: &FocusHandle,
    window: &mut Window,
    cx: &mut gpui::App,
    backward: bool,
) {
    if backward {
        window.focus_prev();
    } else {
        window.focus_next();
    }
    if sheet_child_focused(sheet, window, cx) {
        return;
    }
    if backward {
        focus_last_sheet_stop(sheet, window, cx);
    } else {
        focus_first_sheet_stop(sheet, window, cx);
    }
}

fn sheet_child_focused(sheet: &FocusHandle, window: &Window, cx: &gpui::App) -> bool {
    sheet.contains_focused(window, cx) && !sheet.is_focused(window)
}

fn focus_first_sheet_stop(sheet: &FocusHandle, window: &mut Window, cx: &gpui::App) {
    sheet.focus(window);
    window.focus_next();
    if !sheet_child_focused(sheet, window, cx) {
        sheet.focus(window);
    }
}

fn focus_last_sheet_stop(sheet: &FocusHandle, window: &mut Window, cx: &mut gpui::App) {
    sheet.focus(window);
    let mut last = None;
    for _ in 0..48 {
        window.focus_next();
        if sheet_child_focused(sheet, window, cx) {
            last = window.focused(cx);
        } else {
            break;
        }
    }
    if let Some(handle) = last {
        handle.focus(window);
    }
}
