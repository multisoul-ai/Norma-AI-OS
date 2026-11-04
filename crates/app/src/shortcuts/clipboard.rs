//! Clipboard and modifier admission shared by focused and keyboard-selected nodes.
use gpui::Keystroke;

pub(crate) fn is_paste_shortcut(key: &Keystroke) -> bool {
    let modifiers = key.modifiers;
    key.key == "v"
        && !modifiers.alt
        && !modifiers.shift
        && !modifiers.function
        && if cfg!(windows) {
            modifiers.control && !modifiers.platform
        } else {
            modifiers.platform && !modifiers.control
        }
}

pub(crate) fn selection_accepts_modifiers(key: &Keystroke) -> bool {
    let modifiers = key.modifiers;
    is_paste_shortcut(key) || (!modifiers.platform && !modifiers.control && !modifiers.function)
}
