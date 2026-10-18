//! Application-level GPUI actions shared by the menu bar, shortcuts, and key bindings.

use gpui::actions;

actions!(
    multisoul,
    [
        NewWindow,
        NewTerminal,
        NewClaudeAgent,
        NewCodexAgent,
        NewCursorAgent,
        NewGrokAgent,
        NewKimiAgent,
        NewZCodeAgent,
        NewPiAgent,
        NewHermesAgent,
        NewOpenCodeAgent,
        NewBrowser,
        NewEgo,
        CopyCardContent,
        OpenPreviewFile,
        OpenProject,
        CloneProject,
        OpenFeatureGuide,
        CheckForUpdates,
        ExitFocus,
        ReflowLayout,
        ToggleMasterLayout,
        ToggleSettings,
        QuitNormaOs
    ]
);
