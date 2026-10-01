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
