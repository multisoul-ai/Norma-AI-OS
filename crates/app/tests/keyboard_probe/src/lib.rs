//! Exercise the production App registry with native GPUI keystrokes.
#[path = "../../../src/shortcuts.rs"]
mod shortcuts;

use gpui::Keystroke;
use shortcuts::{ShortcutCommandId, ShortcutSettingsState};

/// 场景：按住默认派发修饰键时，提示必须匹配当前平台的生产命令。
/// 数据构造：Windows Alt派给已有卡片、Ctrl+Shift新建；macOS保持Ctrl与Ctrl+Shift。
/// 执行过程：调用实际渲染使用的badge_mode，包含控制/系统键混合及零待办反例。
/// 预期结果：只为可执行的默认派发显示提示，Windows裸Ctrl不显示派发序号。
#[test]
fn native_idea_badges_match_assignment_modifiers() {
    use shortcuts::idea_assign::{badge_mode, BadgeMode};
    let cards = if cfg!(windows) {
        (false, false, true)
    } else {
        (true, false, false)
    };
    assert_eq!(
        badge_mode(cards.0, cards.1, cards.2, false, 2),
        BadgeMode::Cards,
        "默认已有卡片派发键必须显示序号"
    );
    assert_eq!(
        badge_mode(true, true, false, false, 2),
        BadgeMode::NewAgent,
        "Ctrl+Shift必须显示新建类型"
    );
    for (control, shift, alt, platform, count) in [
        (false, false, false, false, 2),
        (true, false, true, false, 2),
        (true, true, false, true, 2),
        (cards.0, cards.1, cards.2, false, 0),
    ] {
        assert_eq!(
            badge_mode(control, shift, alt, platform, count),
            BadgeMode::Hidden,
            "混合修饰键或无待办不能展示默认提示"
        );
    }
    if cfg!(windows) {
        assert_eq!(
            badge_mode(true, false, false, false, 2),
            BadgeMode::Hidden,
            "Windows裸Ctrl用于创建卡片，不能显示Alt派发提示"
        );
        assert_eq!(
            badge_mode(false, true, true, false, 2),
            BadgeMode::Hidden,
            "Alt+Shift不是默认派发键"
        );
    }
}

/// 场景：新建智能体提示应展示平台默认键及准确类型。
/// 数据构造：默认类型Codex和2/3/9槽位。
/// 执行过程：读取生产渲染使用的new_agent_hint。
/// 预期结果：Windows使用Ctrl文字，macOS使用原生符号；槽位类型不变。
#[test]
fn native_idea_hint_uses_platform_key_labels() {
    let hint = shortcuts::idea_assign::new_agent_hint(norma::protocol::AgentProfile::Codex);
    let (default, typed) = if cfg!(windows) {
        ("Ctrl+0 Codex", "Ctrl+Shift+")
    } else {
        ("⌃0 Codex", "⌃⇧")
    };
    assert!(
        hint.contains(default) && hint.contains(typed),
        "提示必须匹配平台默认键: {hint}"
    );
    assert!(
        hint.contains("2 Claude") && hint.contains("3 Codex") && hint.contains("9 Hermes"),
        "类型必须匹配生产registry: {hint}"
    );
    if cfg!(windows) {
        assert!(
            !hint.contains('⌃'),
            "Windows提示不能残留macOS修饰键符号: {hint}"
        );
    }
}

/// 场景：原生粘贴必须进入已聚焦节点的剪贴板路径。
/// 数据构造：Windows的Ctrl+V、macOS的Cmd+V与裸V、系统键、控制键反例。
/// 执行过程：调用生产focused路径的粘贴判定。
/// 预期结果：只接受精确原生组合；裸V、终端Ctrl+C和其他修饰组合不被粘贴截获。
#[test]
fn native_clipboard_shortcut_reaches_focused_node() {
    use shortcuts::clipboard::is_paste_shortcut;
    let source = if cfg!(windows) { "ctrl-v" } else { "cmd-v" };
    let paste = Keystroke::parse(source).expect("原生粘贴按键合法");
    assert!(
        is_paste_shortcut(&paste),
        "{source}必须进入聚焦节点的剪贴板路径"
    );
    for source in [
        "v",
        "ctrl-c",
        "ctrl-shift-v",
        "ctrl-alt-v",
        "cmd-ctrl-v",
        "cmd-shift-v",
        "cmd-alt-v",
        "cmd-fn-v",
    ] {
        let key = Keystroke::parse(source).expect("反例按键合法");
        assert!(!is_paste_shortcut(&key), "{source}不能被当成原生粘贴");
    }
    if cfg!(windows) {
        let system = Keystroke::parse("cmd-v").expect("系统键反例合法");
        assert!(!is_paste_shortcut(&system), "Win+V必须保留给系统剪贴板历史");
    }
}

/// 场景：方向键选中的节点必须接受原生粘贴，但不能因其他控制组合自动聚焦。
/// 数据构造：平台粘贴、Ctrl+C和附带Shift/Alt/系统/Fn修饰的V。
/// 执行过程：独立调用生产selection修饰键准入，避免被focused路径的首个失败遮蔽。
/// 预期结果：原生粘贴通过外层；其他控制组合拒绝，不能聚焦后向PTY发送0x16。
#[test]
fn native_clipboard_shortcut_enters_keyboard_selection() {
    use shortcuts::clipboard::selection_accepts_modifiers;
    let source = if cfg!(windows) { "ctrl-v" } else { "cmd-v" };
    let paste = Keystroke::parse(source).expect("原生粘贴按键合法");
    assert!(
        selection_accepts_modifiers(&paste),
        "{source}必须通过选中节点外层准入"
    );
    for source in [
        "ctrl-c",
        "ctrl-shift-v",
        "ctrl-alt-v",
        "cmd-ctrl-v",
        "cmd-shift-v",
        "cmd-alt-v",
        "cmd-fn-v",
    ] {
        let key = Keystroke::parse(source).expect("反例按键合法");
        assert!(
            !selection_accepts_modifiers(&key),
            "{source}不能自动聚焦选中节点"
        );
    }
    if cfg!(windows) {
        let system = Keystroke::parse("cmd-v").expect("系统键反例合法");
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
