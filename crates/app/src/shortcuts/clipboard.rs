//! Clipboard and modifier admission shared by focused and keyboard-selected nodes.
use gpui::Keystroke;

pub(crate) fn is_paste_shortcut(key: &Keystroke) -> bool {
    key.modifiers.platform && key.key == "v"
}

pub(crate) fn selection_accepts_modifiers(key: &Keystroke) -> bool {
    let modifiers = key.modifiers;
    if modifiers.platform {
        return key.key == "v"
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && !modifiers.function;
    }
    !modifiers.control && !modifiers.function
}
