use super::*;

/// 桌面导航默认键位可解析、不占用终端输入，且支持保存与恢复。
///
/// 数据构造：6 个导航动作 = 工作空间前后 2 + 项目前后 2 + 两级总览 2；
///   使用空偏好与 Cmd+Shift+J 自定义键位，不涉及时间或 token 预算。
/// 执行过程：逐一分派默认键位；将下一个项目录制为自定义键并序列化重载；恢复默认。
/// 预期结果：六个动作正确分派（项目 Cmd+←/→/↑，工作空间 Cmd+Shift+←/→，Cmd+Shift+↑ 为切换工作空间、Cmd+Shift+K 为全部工作空间），裸方向键、Ctrl+Right 与旧的 Ctrl/Alt+Cmd 键位均不命中；恢复后自定义键失效。
#[test]
fn desktop_shortcuts_defaults_record_and_restore() {
    let mut state = ShortcutSettingsState::default();
    let command = |state: &ShortcutSettingsState, key| {
        state.command_for_keystroke(&Keystroke::parse(key).expect("测试键位合法"))
    };
    let name = |key| command(&state, key).map(|id| serde_json::to_string(&id).unwrap());
    assert_eq!(
        name("cmd-shift-left").as_deref(),
        Some("\"previous_workspace\""),
        "左键切上一个工作空间"
    );
    assert_eq!(
        name("cmd-shift-right").as_deref(),
        Some("\"next_workspace\""),
        "右键切下一个工作空间"
    );
    assert_eq!(
        name("cmd-left").as_deref(),
        Some("\"previous_project\""),
        "左键切上一个项目"
    );
    assert_eq!(
        name("cmd-right").as_deref(),
        Some("\"next_project\""),
        "右键切下一个项目"
    );
    assert_eq!(
        name("cmd-up").as_deref(),
        Some("\"show_project_overview\""),
        "向上返回项目总览"
    );
    assert_eq!(
        name("cmd-shift-up").as_deref(),
        Some("\"switch_workspace\""),
        "⇧⌘↑ 打开工作空间切换器"
    );
    assert_eq!(
        name("cmd-shift-k").as_deref(),
        Some("\"show_workspace_directory\""),
        "⇧⌘K 返回空间目录（与切换器键位互换）"
    );
    assert_eq!(command(&state, "left"), None, "不能抢占裸方向键");
    assert_eq!(
        command(&state, "ctrl-right"),
        None,
        "不能抢占终端 Ctrl+Right"
    );
    for old in [
        "ctrl-cmd-left",
        "ctrl-cmd-right",
        "alt-cmd-left",
        "alt-cmd-right",
        "alt-cmd-up",
        "ctrl-cmd-up",
    ] {
        assert_eq!(
            command(&state, old),
            None,
            "旧键位 {old} 不应再命中任何命令"
        );
    }
    let next = command(&state, "cmd-right").expect("下一个项目命令已注册");
    state.begin_recording(next);
    assert!(
        state.record_keystroke(&Keystroke::parse("cmd-shift-j").unwrap()),
        "应录制新的组合键"
    );
    let mut restored =
        ShortcutSettingsState::new(ShortcutPreferences::from_json(&state.prefs.to_json()), None);
    assert_eq!(
        command(&restored, "cmd-shift-j"),
        Some(next),
        "序列化重载应保留自定义键"
    );
    assert_eq!(command(&restored, "cmd-right"), None, "自定义后旧键应失效");
    restored.restore_default(next);
    assert_eq!(
        command(&restored, "cmd-right"),
        Some(next),
        "恢复后默认键应生效"
    );
    assert_eq!(
        command(&restored, "cmd-shift-j"),
        None,
        "恢复后自定义键不得残留"
    );
}

/// 创建与添加入口有独立默认键位，可持久化重映射且恢复后旧自定义键失效。
///
/// 数据构造：默认命令表、两个新命令，创建改为Cmd+Shift+J，添加改为Cmd+Shift+H。
/// 执行过程：查询默认键与可编辑项 → 检查重复分配 → 录制两个键 → JSON重载 → 全部恢复。
/// 预期结果：Cmd+N/Shift+Cmd+A正确命中；不覆盖新窗口或打开目录；新键重载有效且恢复后无残留。
#[test]
fn desktop_creation_shortcuts_record_and_restore() {
    let mut state = ShortcutSettingsState::default();
    let command = |state: &ShortcutSettingsState, key| {
        state.command_for_keystroke(&Keystroke::parse(key).expect("键位有效"))
    };
    let create = command(&state, "cmd-n").expect("创建工作空间必须有默认快捷键");
    let add = command(&state, "cmd-shift-a").expect("添加项目必须有默认快捷键");
    assert_eq!(
        serde_json::to_string(&create).unwrap(),
        "\"new_workspace\"",
        "Cmd+N必须创建空间而非窗口"
    );
    assert_eq!(
        serde_json::to_string(&add).unwrap(),
        "\"add_project\"",
        "添加键必须打开项目选择器"
    );
    assert_ne!(
        create,
        ShortcutCommandId::NewWindow,
        "创建空间不能变成新窗口"
    );
    assert_ne!(
        add,
        ShortcutCommandId::OpenProject,
        "添加项目不能跳过最近项目选择器"
    );
    assert!(
        state
            .effective
            .iter()
            .find(|e| e.command == create)
            .unwrap()
            .editable,
        "创建键必须出现在可编辑设置中"
    );
    assert!(
        state
            .effective
            .iter()
            .find(|e| e.command == add)
            .unwrap()
            .editable,
        "添加键必须出现在可编辑设置中"
    );
    assert!(
        validate_shortcut_change(add, "cmd-n", &state.effective).is_err(),
        "设置必须拦截与创建键冲突"
    );
    state.begin_recording(create);
    assert!(
        state.record_keystroke(&Keystroke::parse("cmd-shift-j").unwrap()),
        "创建键应可录制"
    );
    state.begin_recording(add);
    assert!(
        state.record_keystroke(&Keystroke::parse("cmd-shift-h").unwrap()),
        "添加键应可录制"
    );
    let mut loaded =
        ShortcutSettingsState::new(ShortcutPreferences::from_json(&state.prefs.to_json()), None);
    assert_eq!(
        command(&loaded, "cmd-shift-j"),
        Some(create),
        "创建自定义键应持久化"
    );
    assert_eq!(
        command(&loaded, "cmd-shift-h"),
        Some(add),
        "添加自定义键应持久化"
    );
    assert_eq!(command(&loaded, "cmd-n"), None, "重映射后默认创建键应失效");
    assert_eq!(
        command(&loaded, "cmd-shift-a"),
        None,
        "重映射后默认添加键应失效"
    );
    loaded.restore_all_defaults();
    assert_eq!(command(&loaded, "cmd-n"), Some(create), "恢复默认创建键");
    assert_eq!(command(&loaded, "cmd-shift-a"), Some(add), "恢复默认添加键");
    assert_eq!(
        command(&loaded, "cmd-shift-j"),
        None,
        "恢复后创建自定义键不能残留"
    );
    assert_eq!(
        command(&loaded, "cmd-shift-h"),
        None,
        "恢复后添加自定义键不能残留"
    );
}

/// 切换运行概览默认是 ⌘⇧E，可录制成其他组合并恢复，且不改语音的 ⌘E。
///
/// 数据构造：空偏好；自定义键选未被占用的 Cmd+Shift+Y。
/// 执行过程：核对默认分派，录制新键，JSON 重载，再恢复默认。
/// 预期结果：默认 ⌘⇧E 命中切换运行概览，⌘E 始终是语音；恢复后自定义键消失。
#[test]
fn toggle_run_overview_records_and_restores_without_taking_voice() {
    let mut state = ShortcutSettingsState::default();
    let command = |state: &ShortcutSettingsState, key| {
        state.command_for_keystroke(&Keystroke::parse(key).expect("测试键位合法"))
    };
    assert_eq!(
        command(&state, "cmd-shift-e"),
        Some(ShortcutCommandId::ToggleRunOverview),
        "⌘⇧E 应切换运行概览"
    );
    assert_eq!(
        command(&state, "cmd-e"),
        Some(ShortcutCommandId::VoiceInteraction),
        "⌘E 仍是语音，不能被运行概览占用"
    );
    state.begin_recording(ShortcutCommandId::ToggleRunOverview);
    assert!(
        state.record_keystroke(&Keystroke::parse("cmd-shift-y").unwrap()),
        "应能把切换运行概览录制成 ⌘⇧Y"
    );
    let mut restored =
        ShortcutSettingsState::new(ShortcutPreferences::from_json(&state.prefs.to_json()), None);
    assert_eq!(
        command(&restored, "cmd-shift-y"),
        Some(ShortcutCommandId::ToggleRunOverview),
        "重载后自定义键仍切换运行概览"
    );
    assert_eq!(
        command(&restored, "cmd-shift-e"),
        None,
        "录制新键后默认 ⌘⇧E 应让出"
    );
    assert_eq!(
        command(&restored, "cmd-e"),
        Some(ShortcutCommandId::VoiceInteraction),
        "改运行概览的键不应影响语音 ⌘E"
    );
    restored.restore_default(ShortcutCommandId::ToggleRunOverview);
    assert_eq!(
        command(&restored, "cmd-shift-e"),
        Some(ShortcutCommandId::ToggleRunOverview),
        "恢复默认后 ⌘⇧E 回到切换运行概览"
    );
    assert_eq!(
        command(&restored, "cmd-shift-y"),
        None,
        "恢复默认后自定义键不再命中"
    );
    assert_eq!(
        command(&restored, "cmd-e"),
        Some(ShortcutCommandId::VoiceInteraction),
        "恢复运行概览默认键后语音仍是 ⌘E"
    );
}

/// 「想法」视图快捷键：默认 ⌘⇧I，可在 Settings 录制、保存重载并恢复，且不与既有键冲突。
///
/// 数据构造：空偏好；录制自定义键 ⌘⇧U；尝试把它录制成运行概览的 ⌘⇧E 与语音的 ⌘E。
/// 执行过程：解析默认键；录制、序列化重载；尝试冲突键位；恢复默认。
/// 预期结果：
///   - 正断言：⌘⇧I 命中想法；录制 ⌘⇧U 后重载仍命中，默认键让出；恢复后 ⌘⇧I 回来；该条目可编辑。
///   - 负断言：⌘⇧E 仍是运行概览、⌘E 仍是语音；想法不能被录制成这两个已占用的键，也不能是裸 I。
#[test]
fn toggle_ideas_shortcut_defaults_records_and_rejects_conflicts() {
    let mut state = ShortcutSettingsState::default();
    let command = |state: &ShortcutSettingsState, key| {
        state.command_for_keystroke(&Keystroke::parse(key).expect("测试键位合法"))
    };
    assert_eq!(
        command(&state, "cmd-shift-i"),
        Some(ShortcutCommandId::ToggleIdeas),
        "⌘⇧I 应切换想法视图"
    );
    assert_eq!(
        command(&state, "cmd-shift-e"),
        Some(ShortcutCommandId::ToggleRunOverview),
        "⌘⇧E 仍是运行概览"
    );
    assert_eq!(
        command(&state, "cmd-e"),
        Some(ShortcutCommandId::VoiceInteraction),
        "⌘E 仍是语音"
    );
    let effective = effective_shortcuts(&state.prefs);
    assert!(
        effective
            .iter()
            .find(|entry| entry.command == ShortcutCommandId::ToggleIdeas)
            .is_some_and(|entry| entry.editable),
        "想法快捷键必须出现在 Settings 且可编辑"
    );
    for taken in ["cmd-shift-e", "cmd-e", "i"] {
        assert!(
            validate_shortcut_change(ShortcutCommandId::ToggleIdeas, taken, &effective).is_err(),
            "想法不能被改成已占用或裸键 {taken}"
        );
    }
    assert!(
        validate_shortcut_change(ShortcutCommandId::ToggleIdeas, "cmd-shift-u", &effective).is_ok(),
        "想法应允许改成未被占用的 ⌘⇧U"
    );
    state.begin_recording(ShortcutCommandId::ToggleIdeas);
    assert!(
        state.record_keystroke(&Keystroke::parse("cmd-shift-u").unwrap()),
        "应能录制成 ⌘⇧U"
    );
    let mut restored =
        ShortcutSettingsState::new(ShortcutPreferences::from_json(&state.prefs.to_json()), None);
    assert_eq!(
        command(&restored, "cmd-shift-u"),
        Some(ShortcutCommandId::ToggleIdeas),
        "重载后自定义键仍切换想法"
    );
    assert_eq!(
        command(&restored, "cmd-shift-i"),
        None,
        "录制新键后默认 ⌘⇧I 应让出"
    );
    restored.restore_default(ShortcutCommandId::ToggleIdeas);
    assert_eq!(
        command(&restored, "cmd-shift-i"),
        Some(ShortcutCommandId::ToggleIdeas),
        "恢复默认后 ⌘⇧I 回来"
    );
    assert_eq!(
        command(&restored, "cmd-shift-u"),
        None,
        "恢复默认后自定义键不再命中"
    );
}

/// ⌘⇧B 打开大脑活动窗口，可在 Settings 里录制和恢复，且不与已有键位冲突。
///
/// 执行过程：解析默认键；检查条目可编辑；尝试已占用的键位。
/// 预期结果：
///   - 正断言：⌘⇧B 命中 ToggleBrainTrace；条目出现在 Settings 且可编辑；可改成未占用的 ⌘⇧U
///   - 负断言：⌘⇧E 仍是运行概览；不能被改成已占用的 ⌘⇧E
#[test]
fn toggle_brain_trace_shortcut_is_configurable_and_conflict_free() {
    let state = ShortcutSettingsState::default();
    let command = |key| state.command_for_keystroke(&Keystroke::parse(key).expect("测试键位合法"));
    assert_eq!(
        command("cmd-shift-b"),
        Some(ShortcutCommandId::ToggleBrainTrace)
    );
    assert_eq!(
        command("cmd-shift-e"),
        Some(ShortcutCommandId::ToggleRunOverview)
    );
    let effective = effective_shortcuts(&state.prefs);
    assert!(effective
        .iter()
        .find(|entry| entry.command == ShortcutCommandId::ToggleBrainTrace)
        .is_some_and(|entry| entry.editable));
    assert!(validate_shortcut_change(
        ShortcutCommandId::ToggleBrainTrace,
        "cmd-shift-e",
        &effective
    )
    .is_err());
    assert!(validate_shortcut_change(
        ShortcutCommandId::ToggleBrainTrace,
        "cmd-shift-u",
        &effective
    )
    .is_ok());
}
