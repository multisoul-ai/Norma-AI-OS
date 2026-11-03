//! Exercise the production App registry with native GPUI keystrokes.
#[path = "../../../src/shortcuts.rs"]
mod shortcuts;

use gpui::Keystroke;
use shortcuts::{ShortcutCommandId, ShortcutSettingsState};

/// 场景：粘贴既可作用于已聚焦节点，也能进入方向键选中的节点。
/// 数据构造：Windows的Ctrl+V、macOS的Cmd+V与裸V、系统键、控制键反例。
/// 执行过程：调用生产focused路径的粘贴判定和selection路径的修饰键准入。
/// 预期结果：原生粘贴同时通过两层；终端Ctrl+C不被粘贴截获，也不触发选中卡片自动聚焦。
#[test]
fn native_clipboard_shortcut_reaches_focused_and_selected_nodes() {
    use shortcuts::clipboard::{is_paste_shortcut, selection_accepts_modifiers};
    let source = if cfg!(windows) { "ctrl-v" } else { "cmd-v" };
    let paste = Keystroke::parse(source).expect("原生粘贴按键合法");
    assert!(
        is_paste_shortcut(&paste),
        "{source}必须进入聚焦节点的剪贴板路径"
    );
    assert!(
        selection_accepts_modifiers(&paste),
        "{source}必须通过选中节点的外层准入"
    );
    for source in ["v", "ctrl-c", "ctrl-shift-v", "ctrl-alt-v", "cmd-ctrl-v"] {
        let key = Keystroke::parse(source).expect("反例按键合法");
        assert!(!is_paste_shortcut(&key), "{source}不能被当成原生粘贴");
    }
    let interrupt = Keystroke::parse("ctrl-c").expect("终端中断按键合法");
    assert!(
        !selection_accepts_modifiers(&interrupt),
        "Ctrl+C不能自动聚焦选中卡片"
    );
    if cfg!(windows) {
        let system = Keystroke::parse("cmd-v").expect("系统键反例合法");
        assert!(!is_paste_shortcut(&system), "Win+V必须保留给系统剪贴板历史");
        assert!(
            !selection_accepts_modifiers(&system),
            "Win+V不能自动聚焦卡片"
        );
    }
}

/// 场景：原生桌面用户以平台主修饰键新建工作区。
/// 数据构造：无偏好覆盖的生产注册表；macOS使用Cmd+N，Windows使用Ctrl+N。
/// 执行过程：解析真实GPUI按键并经生产command_for_keystroke分派。
/// 预期结果：命中新建工作区，不被Windows系统键或其他App命令替代。
#[test]
fn native_workspace_shortcut_uses_platform_primary() {
    let source = if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-n"
    };
    let key = Keystroke::parse(source).expect("平台默认快捷键必须可解析");
    let state = ShortcutSettingsState::default();
    assert_eq!(
        state.command_for_keystroke(&key),
        Some(ShortcutCommandId::NewWorkspace),
        "平台主修饰键必须触发新建工作区"
    );
    assert_eq!(
        state.command_for_keystroke(&Keystroke::parse("n").expect("裸键合法")),
        None,
        "输入工作区名称时裸字母不能触发App命令"
    );
}

/// 场景：Windows默认命令须采用原生按键，且不抢占PTY基本控制输入。
/// 数据构造：生产默认registry；Windows为Ctrl命令、Alt数字派发、F6退焦和F8语音。
/// 执行过程：逐个以真实GPUI按键查询命令；遍历全部默认键检测唯一性、可分派性和modified状态。
/// 预期结果：创建/复制/删除/导航正确且默认互不冲突；Ctrl+C/D/M/E和Ctrl方向键继续留给终端。
#[test]
fn native_registry_defaults_dispatch_uniquely_and_preserve_terminal_controls() {
    let state = ShortcutSettingsState::default();
    if cfg!(windows) {
        let cases = [
            ("ctrl-1", ShortcutCommandId::NewTerminal),
            ("ctrl-2", ShortcutCommandId::NewClaudeAgent),
            ("ctrl-3", ShortcutCommandId::NewCodexAgent),
            ("ctrl-shift-c", ShortcutCommandId::CopyCardContent),
            ("ctrl-shift-d", ShortcutCommandId::DeleteCard),
            ("ctrl-shift-m", ShortcutCommandId::ToggleMasterLayout),
            ("ctrl-o", ShortcutCommandId::OpenProject),
            ("ctrl-s", ShortcutCommandId::SavePreviewFile),
            ("ctrl-,", ShortcutCommandId::ToggleSettings),
            ("ctrl-pageup", ShortcutCommandId::PreviousProject),
            ("ctrl-pagedown", ShortcutCommandId::NextProject),
            ("ctrl-shift-pageup", ShortcutCommandId::PreviousWorkspace),
            ("ctrl-shift-pagedown", ShortcutCommandId::NextWorkspace),
            ("alt-up", ShortcutCommandId::ShowProjectOverview),
            ("alt-down", ShortcutCommandId::ExpandSelectedProject),
            ("alt-1", ShortcutCommandId::AssignIdeaToCard(1)),
            ("ctrl-shift-3", ShortcutCommandId::AssignIdeaToNewAgent(3)),
            ("f6", ShortcutCommandId::ExitFocus),
            ("f8", ShortcutCommandId::VoiceInteraction),
        ];
        for (key, expected) in cases {
            assert_eq!(
                state.command_for_keystroke(&Keystroke::parse(key).expect("原生键位合法")),
                Some(expected),
                "Windows默认键位{key}必须分派到对应命令"
            );
        }
        for key in [
            "ctrl-c",
            "ctrl-d",
            "ctrl-m",
            "ctrl-e",
            "ctrl-left",
            "ctrl-right",
        ] {
            assert_eq!(
                state.command_for_keystroke(&Keystroke::parse(key).expect("终端控制键合法")),
                None,
                "Windows默认命令不能吞掉终端控制键{key}"
            );
        }
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &state.effective {
        assert!(!entry.modified, "无覆盖的默认键位不能显示为已修改");
        let Some(key) = &entry.current_keystroke else {
            continue;
        };
        let parsed = Keystroke::parse(key).expect("全部默认键位均可解析");
        assert!(seen.insert(parsed.unparse()), "默认键位不能互相覆盖: {key}");
        assert_eq!(
            state.command_for_keystroke(&parsed),
            Some(entry.command),
            "默认键{key}必须能经生产canonical匹配分派"
        );
        if cfg!(windows) {
            assert!(
                !parsed.modifiers.platform,
                "Windows默认命令不能绑定系统Win键: {key}"
            );
        }
    }
}
