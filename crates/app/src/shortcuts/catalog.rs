use super::{ReadOnlyShortcutBehavior, ShortcutCommandId, ShortcutEntry};

const DEFAULT_SHORTCUT_ENTRIES: &[ShortcutEntry] = &[
        entry(ShortcutCommandId::SavePreviewFile, "Cards", "Save File", "Save edits in the active file card", Some("cmd-s")),
        entry(ShortcutCommandId::NewWindow, "Workspace", "New Window", "Open a new project launcher window", Some("cmd-shift-n")),
        entry(ShortcutCommandId::NewTerminal, "Workspace", "New Terminal", "Create a terminal card in the visible canvas", Some("cmd-1")),
        entry(ShortcutCommandId::NewBrowser, "Workspace", "New Browser", "Create a lightweight browser card in the visible canvas", None),
        entry(ShortcutCommandId::OpenProject, "Workspace", "Open Project", "Choose a workspace folder", Some("cmd-o")),
        entry(ShortcutCommandId::OpenPreviewFile, "Workspace", "Open Preview File", "Create a preview card from a local file", Some("cmd-shift-o")),
        entry(ShortcutCommandId::NewClaudeAgent, "Cards", "New Claude Agent", "Create a Claude Code live agent card", Some("cmd-2")),
        entry(ShortcutCommandId::NewCodexAgent, "Cards", "New Codex Agent", "Create a Codex live agent card", Some("cmd-3")),
        entry(ShortcutCommandId::NewCursorAgent, "Cards", "New Cursor Agent", "Create a Cursor Agent live agent card", Some("cmd-4")),
        entry(ShortcutCommandId::NewOpenCodeAgent, "Cards", "New OpenCode Agent", "Create an OpenCode live agent card", Some("cmd-5")),
        entry(ShortcutCommandId::NewGrokAgent, "Cards", "New Grok Agent", "Create a Grok Build live agent card", Some("cmd-6")),
        entry(ShortcutCommandId::CopyCardContent, "Cards", "Copy Selection", "Copy the active terminal text selection", Some("cmd-c")),
        entry(ShortcutCommandId::DeleteCard, "Cards", "Delete Card", "Delete the focused or single selected card", Some("cmd-d")),
        entry(ShortcutCommandId::ReflowLayout, "Layout", "Reflow Layout", "Tile all cards in the visible workspace", Some("cmd-shift-l")),
        entry(ShortcutCommandId::ToggleMasterLayout, "Layout", "Toggle Master Layout", "Switch between master/detail and tiled layout", Some("cmd-m")),
        entry(ShortcutCommandId::ExitFocus, "Layout", "Exit Focus", "Return keyboard control to the canvas. When a card is focused, bare Esc always reaches the terminal — use this shortcut or click outside to exit.", Some("cmd-escape")),
        entry(ShortcutCommandId::ToggleSettings, "Settings", "Open Settings", "Show or hide the Settings panel", Some("cmd-,")),
        entry(ShortcutCommandId::VoiceInteraction, "Voice", "Voice Interaction", "Start or stop the active voice interaction mode", Some("cmd-e")),
    ];

pub(crate) fn default_shortcut_entries() -> &'static [ShortcutEntry] {
    DEFAULT_SHORTCUT_ENTRIES
}

const fn entry(
    command: ShortcutCommandId,
    category: &'static str,
    title: &'static str,
    hint: &'static str,
    default_keystroke: Option<&'static str>,
) -> ShortcutEntry {
    ShortcutEntry {
        command,
        category,
        title,
        hint,
        default_keystroke,
        editable: true,
    }
}

const READ_ONLY_BEHAVIORS: &[ReadOnlyShortcutBehavior] = &[
    ReadOnlyShortcutBehavior {
        title: "Paste",
        hint: "Send clipboard text to the focused or keyboard-selected node",
        keystroke: "cmd-v",
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
];

pub(crate) fn read_only_behaviors() -> &'static [ReadOnlyShortcutBehavior] {
    READ_ONLY_BEHAVIORS
}
