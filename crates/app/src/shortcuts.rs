//! App command shortcut registry, preferences, and validation.

use gpui::Keystroke;
use serde::{Deserialize, Serialize};
use std::fmt;
use storage::{AppStore, SettingsStore};

const SHORTCUT_PREFS_KEY: &str = "keyboard.shortcuts.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShortcutCommandId {
    NewWindow,
    NewTerminal,
    CopyCardContent,
    OpenProject,
    OpenPreviewFile,
    NewClaudeAgent,
    NewCodexAgent,
    NewOpenCodeAgent,
    DeleteCard,
    ReflowLayout,
    ToggleMasterLayout,
    ExitFocus,
    ToggleSettings,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ShortcutPreferences {
    #[serde(default)]
    overrides: Vec<ShortcutOverride>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ShortcutOverride {
    command: ShortcutCommandId,
    keystroke: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShortcutEntry {
    pub(crate) command: ShortcutCommandId,
    pub(crate) category: &'static str,
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) default_keystroke: Option<&'static str>,
    pub(crate) editable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectiveShortcutEntry {
    pub(crate) command: ShortcutCommandId,
    pub(crate) category: &'static str,
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) default_keystroke: Option<&'static str>,
    pub(crate) current_keystroke: Option<String>,
    pub(crate) editable: bool,
    pub(crate) modified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReadOnlyShortcutBehavior {
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) keystroke: &'static str,
    pub(crate) scope: &'static str,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShortcutStatus {
    pub(crate) message: Option<String>,
    pub(crate) is_error: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ShortcutSettingsState {
    pub(crate) prefs: ShortcutPreferences,
    pub(crate) effective: Vec<EffectiveShortcutEntry>,
    pub(crate) recording: Option<ShortcutCommandId>,
    pub(crate) status: ShortcutStatus,
    store: Option<SettingsStore>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutValidationError {
    UnknownCommand,
    NotEditable(&'static str),
    InvalidKeystroke(String),
    ReservedInput(&'static str),
    DuplicateCommand(&'static str),
    BarePrintable,
}

impl ShortcutPreferences {
    pub(crate) fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"overrides\":[]}".to_string())
    }

    pub(crate) fn set_override(&mut self, command: ShortcutCommandId, keystroke: String) {
        self.restore_default(command);
        self.overrides.push(ShortcutOverride { command, keystroke });
    }

    pub(crate) fn restore_default(&mut self, command: ShortcutCommandId) {
        self.overrides.retain(|entry| entry.command != command);
    }

    pub(crate) fn restore_all_defaults(&mut self) {
        self.overrides.clear();
    }

    fn override_for(&self, command: ShortcutCommandId) -> Option<&str> {
        self.overrides
            .iter()
            .rev()
            .find(|entry| entry.command == command)
            .map(|entry| entry.keystroke.as_str())
    }
}

impl Default for ShortcutSettingsState {
    fn default() -> Self {
        Self::new(ShortcutPreferences::default(), None)
    }
}

impl ShortcutSettingsState {
    pub(crate) fn load_default() -> Self {
        let store = AppStore::open_default()
            .ok()
            .map(|app| app.settings_store());
        let prefs = store
            .as_ref()
            .and_then(|store| store.get_string(SHORTCUT_PREFS_KEY).ok().flatten())
            .map(|raw| ShortcutPreferences::from_json(&raw))
            .unwrap_or_default();
        Self::new(prefs, store)
    }

    fn new(prefs: ShortcutPreferences, store: Option<SettingsStore>) -> Self {
        let effective = effective_shortcuts(&prefs);
        Self {
            prefs,
            effective,
            recording: None,
            status: ShortcutStatus::default(),
            store,
        }
    }

    pub(crate) fn begin_recording(&mut self, command: ShortcutCommandId) {
        self.recording = Some(command);
        self.status = ShortcutStatus {
            message: Some("Type a new shortcut".to_string()),
            is_error: false,
        };
    }

    pub(crate) fn restore_default(&mut self, command: ShortcutCommandId) {
        self.prefs.restore_default(command);
        self.refresh_and_save("Shortcut restored");
    }

    pub(crate) fn restore_all_defaults(&mut self) {
        self.prefs.restore_all_defaults();
        self.refresh_and_save("Keyboard shortcuts restored");
    }

    pub(crate) fn command_for_keystroke(&self, keystroke: &Keystroke) -> Option<ShortcutCommandId> {
        let source = keystroke.unparse();
        self.effective.iter().find_map(|entry| {
            (entry.current_keystroke.as_deref() == Some(source.as_str())).then_some(entry.command)
        })
    }

    pub(crate) fn record_keystroke(&mut self, keystroke: &Keystroke) -> bool {
        let Some(command) = self.recording else {
            return false;
        };

        if command != ShortcutCommandId::ExitFocus
            && keystroke.key == "escape"
            && !keystroke.modifiers.modified()
        {
            self.recording = None;
            self.status = ShortcutStatus {
                message: Some("Recording cancelled".to_string()),
                is_error: false,
            };
            return true;
        }

        let source = keystroke.unparse();
        match validate_shortcut_change(command, &source, &self.effective) {
            Ok(()) => {
                self.prefs.set_override(command, source);
                self.recording = None;
                self.refresh_and_save("Shortcut saved");
            }
            Err(error) => {
                self.status = ShortcutStatus {
                    message: Some(error.to_string()),
                    is_error: true,
                };
            }
        }
        true
    }

    fn refresh_and_save(&mut self, success_message: &str) {
        self.effective = effective_shortcuts(&self.prefs);
        match self
            .store
            .as_ref()
            .map(|store| store.set_string(SHORTCUT_PREFS_KEY, &self.prefs.to_json()))
        {
            Some(Err(error)) => {
                self.status = ShortcutStatus {
                    message: Some(format!("Could not save shortcuts: {error}")),
                    is_error: true,
                };
            }
            _ => {
                self.status = ShortcutStatus {
                    message: Some(success_message.to_string()),
                    is_error: false,
                };
            }
        }
    }
}

impl fmt::Display for ShortcutValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand => write!(f, "Unknown shortcut command"),
            Self::NotEditable(title) => write!(f, "{title} cannot be edited"),
            Self::InvalidKeystroke(raw) => write!(f, "{raw} is not a valid shortcut"),
            Self::ReservedInput(title) => write!(f, "Reserved by {title}"),
            Self::DuplicateCommand(title) => write!(f, "Already used by {title}"),
            Self::BarePrintable => write!(f, "Use a modifier key for text shortcuts"),
        }
    }
}

pub(crate) fn default_shortcut_entries() -> &'static [ShortcutEntry] {
    &[
        ShortcutEntry {
            command: ShortcutCommandId::NewWindow,
            category: "Workspace",
            title: "New Window",
            hint: "Open a new project launcher window",
            default_keystroke: Some("cmd-shift-n"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::NewTerminal,
            category: "Workspace",
            title: "New Terminal",
            hint: "Create a terminal card in the visible canvas",
            default_keystroke: Some("cmd-n"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::OpenProject,
            category: "Workspace",
            title: "Open Project",
            hint: "Choose a workspace folder",
            default_keystroke: Some("cmd-o"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::OpenPreviewFile,
            category: "Workspace",
            title: "Open Preview File",
            hint: "Create a preview card from a local file",
            default_keystroke: Some("cmd-shift-o"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::NewClaudeAgent,
            category: "Cards",
            title: "New Claude Agent",
            hint: "Create a Claude Code live agent card",
            default_keystroke: None,
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::NewCodexAgent,
            category: "Cards",
            title: "New Codex Agent",
            hint: "Create a Codex live agent card",
            default_keystroke: None,
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::NewOpenCodeAgent,
            category: "Cards",
            title: "New OpenCode Agent",
            hint: "Create an OpenCode live agent card",
            default_keystroke: None,
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::CopyCardContent,
            category: "Cards",
            title: "Copy Selection",
            hint: "Copy the active terminal text selection",
            default_keystroke: Some("cmd-c"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::DeleteCard,
            category: "Cards",
            title: "Delete Card",
            hint: "Delete the focused or single selected card",
            default_keystroke: Some("cmd-d"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::ReflowLayout,
            category: "Layout",
            title: "Reflow Layout",
            hint: "Tile all cards in the visible workspace",
            default_keystroke: Some("cmd-shift-l"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::ToggleMasterLayout,
            category: "Layout",
            title: "Toggle Master Layout",
            hint: "Switch between master/detail and tiled layout",
            default_keystroke: Some("cmd-m"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::ExitFocus,
            category: "Layout",
            title: "Exit Focus",
            hint: "Return keyboard control to the canvas",
            default_keystroke: Some("escape"),
            editable: true,
        },
        ShortcutEntry {
            command: ShortcutCommandId::ToggleSettings,
            category: "Settings",
            title: "Open Settings",
            hint: "Show or hide the Settings panel",
            default_keystroke: Some("cmd-,"),
            editable: true,
        },
    ]
}

pub(crate) fn read_only_behaviors() -> &'static [ReadOnlyShortcutBehavior] {
    &[
        ReadOnlyShortcutBehavior {
            title: "Paste",
            hint: "Send clipboard text to the focused or keyboard-selected node",
            keystroke: "cmd-v",
            scope: "System",
        },
        ReadOnlyShortcutBehavior {
            title: "Voice Recording",
            hint: "Hold to record a voice command",
            keystroke: "cmd-shift-v",
            scope: "System",
        },
        ReadOnlyShortcutBehavior {
            title: "Arrow Navigation",
            hint: "Move selection across visible canvas cards",
            keystroke: "up",
            scope: "Canvas behavior",
        },
        ReadOnlyShortcutBehavior {
            title: "Space Pan",
            hint: "Hold Space and drag to pan the canvas",
            keystroke: "space",
            scope: "Canvas behavior",
        },
        ReadOnlyShortcutBehavior {
            title: "Terminal Input",
            hint: "Printable keys, Enter, Tab, Ctrl, and Alt are forwarded to PTY",
            keystroke: "ctrl-* / alt-*",
            scope: "Node behavior",
        },
    ]
}

pub(crate) fn effective_shortcuts(prefs: &ShortcutPreferences) -> Vec<EffectiveShortcutEntry> {
    default_shortcut_entries()
        .iter()
        .map(|entry| {
            let override_keystroke = prefs
                .override_for(entry.command)
                .and_then(normalize_user_keystroke);
            let current_keystroke =
                override_keystroke.or_else(|| entry.default_keystroke.map(str::to_string));
            EffectiveShortcutEntry {
                command: entry.command,
                category: entry.category,
                title: entry.title,
                hint: entry.hint,
                default_keystroke: entry.default_keystroke,
                modified: current_keystroke.as_deref() != entry.default_keystroke,
                current_keystroke,
                editable: entry.editable,
            }
        })
        .collect()
}

pub(crate) fn validate_shortcut_change(
    command: ShortcutCommandId,
    candidate: &str,
    effective: &[EffectiveShortcutEntry],
) -> Result<(), ShortcutValidationError> {
    let normalized = normalize_user_keystroke(candidate)
        .ok_or_else(|| ShortcutValidationError::InvalidKeystroke(candidate.to_string()))?;
    let parsed = Keystroke::parse(&normalized)
        .map_err(|_| ShortcutValidationError::InvalidKeystroke(candidate.to_string()))?;
    let target = effective
        .iter()
        .find(|entry| entry.command == command)
        .ok_or(ShortcutValidationError::UnknownCommand)?;
    if !target.editable {
        return Err(ShortcutValidationError::NotEditable(target.title));
    }
    if is_bare_printable(&parsed) {
        return Err(ShortcutValidationError::BarePrintable);
    }
    if let Some(behavior) = reserved_behavior_for(&normalized) {
        return Err(ShortcutValidationError::ReservedInput(behavior.title));
    }
    if let Some(conflict) = effective.iter().find(|entry| {
        entry.command != command && entry.current_keystroke.as_deref() == Some(normalized.as_str())
    }) {
        return Err(ShortcutValidationError::DuplicateCommand(conflict.title));
    }
    Ok(())
}

pub(crate) fn keystroke_label(source: Option<&str>) -> String {
    match source {
        Some(source) => Keystroke::parse(source)
            .map(|keystroke| keystroke.to_string())
            .unwrap_or_else(|_| source.to_string()),
        None => "Not Set".to_string(),
    }
}

fn normalize_user_keystroke(source: &str) -> Option<String> {
    let keystroke = Keystroke::parse(source).ok()?;
    if is_modifier_key(&keystroke.key) {
        return None;
    }
    Some(keystroke.unparse())
}

fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "shift" | "control" | "alt" | "platform" | "function" | "cmd" | "ctrl"
    )
}

fn is_bare_printable(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.modified() && keystroke.key.chars().count() == 1
}

fn reserved_behavior_for(normalized: &str) -> Option<&'static ReadOnlyShortcutBehavior> {
    read_only_behaviors().iter().find(|behavior| {
        behavior.keystroke == normalized
            || (behavior.title == "Arrow Navigation"
                && matches!(normalized, "up" | "down" | "left" | "right"))
    })
}

#[cfg(test)]
#[path = "../tests/unit/shortcuts_tests.rs"]
mod tests;
