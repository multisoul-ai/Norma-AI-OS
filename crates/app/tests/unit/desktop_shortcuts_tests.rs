use super::*;

/// 桌面导航默认键位可解析、不占用终端输入，且支持保存与恢复。
///
/// 数据构造：6 个导航动作 = 工作空间前后 2 + 项目前后 2 + 两级总览 2；
///   使用空偏好与 Cmd+Shift+J 自定义键位，不涉及时间或 token 预算。
/// 执行过程：逐一分派默认键位；将下一个项目录制为自定义键并序列化重载；恢复默认。
/// 预期结果：六个动作正确分派，裸方向键和 Ctrl+Right 不命中；恢复后自定义键失效。
#[test]
fn desktop_shortcuts_defaults_record_and_restore() {
    let mut state = ShortcutSettingsState::default();
    let command = |state: &ShortcutSettingsState, key| {
        state.command_for_keystroke(&Keystroke::parse(key).expect("测试键位合法"))
    };
    let name = |key| command(&state, key).map(|id| serde_json::to_string(&id).unwrap());
    assert_eq!(
        name("ctrl-cmd-left").as_deref(),
        Some("\"previous_workspace\""),
        "左键切上一个工作空间"
    );
    assert_eq!(
        name("ctrl-cmd-right").as_deref(),
        Some("\"next_workspace\""),
        "右键切下一个工作空间"
    );
    assert_eq!(
        name("alt-cmd-left").as_deref(),
        Some("\"previous_project\""),
        "左键切上一个项目"
    );
    assert_eq!(
        name("alt-cmd-right").as_deref(),
        Some("\"next_project\""),
        "右键切下一个项目"
    );
    assert_eq!(
        name("alt-cmd-up").as_deref(),
        Some("\"show_project_overview\""),
        "向上返回项目总览"
    );
    assert_eq!(
        name("ctrl-cmd-up").as_deref(),
        Some("\"show_workspace_directory\""),
        "向上返回空间目录"
    );
    assert_eq!(command(&state, "left"), None, "不能抢占裸方向键");
    assert_eq!(
        command(&state, "ctrl-right"),
        None,
        "不能抢占终端 Ctrl+Right"
    );
    let next = command(&state, "alt-cmd-right").expect("下一个项目命令已注册");
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
    assert_eq!(
        command(&restored, "alt-cmd-right"),
        None,
        "自定义后旧键应失效"
    );
    restored.restore_default(next);
    assert_eq!(
        command(&restored, "alt-cmd-right"),
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
