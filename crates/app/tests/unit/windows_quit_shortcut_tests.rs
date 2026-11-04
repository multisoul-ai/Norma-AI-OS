use super::*;

/// 场景：Windows用户需要可发现、可配置的退出命令，不能依赖被系统占用的Win+Q。
/// 构造：生产默认注册表、Ctrl+Shift+Q默认键、Ctrl+Alt+Q用户覆盖与Ctrl+C终端反例。
/// 执行：经正式录制接口修改退出键，再恢复默认，检查每阶段的实际命令分派。
/// 预期：退出命令有可编辑Settings行；覆盖替换默认键，恢复后旧覆盖失效且Ctrl+C不被拦截。
#[test]
fn native_quit_shortcut_is_editable_and_restorable() {
    let mut settings = ShortcutSettingsState::default();
    let entry = settings
        .effective
        .iter()
        .find(|entry| serde_json::to_value(entry.command).unwrap() == "quit_norma_os")
        .expect("Windows退出命令必须注册到生产Settings列表");
    let command = entry.command;
    assert!(entry.editable, "用户必须能在Settings录制和恢复退出键");
    assert_eq!(
        entry.current_keystroke.as_deref(),
        Some("ctrl-shift-q"),
        "默认退出键必须采用可达的Windows组合"
    );
    let default = Keystroke::parse("ctrl-shift-q").unwrap();
    let custom = Keystroke::parse("ctrl-alt-q").unwrap();
    assert_eq!(
        settings.command_for_keystroke(&default),
        Some(command),
        "默认键必须分派退出命令"
    );
    settings.begin_recording(command);
    assert!(
        settings.record_keystroke(&custom),
        "正式录制入口必须接收覆盖键"
    );
    assert!(!settings.status.is_error, "合法覆盖必须有成功状态反馈");
    assert!(settings.recording.is_none(), "录制成功后必须退出录制状态");
    assert_eq!(
        settings.command_for_keystroke(&custom),
        Some(command),
        "覆盖后必须使用录制的新键"
    );
    assert_eq!(
        settings.command_for_keystroke(&default),
        None,
        "修改后默认键不能继续意外退出App"
    );
    settings.restore_default(command);
    assert_eq!(
        settings.command_for_keystroke(&default),
        Some(command),
        "恢复后默认退出键必须重新生效"
    );
    assert_eq!(
        settings.command_for_keystroke(&custom),
        None,
        "恢复后不能保留过期的用户覆盖"
    );
    assert_eq!(
        settings.command_for_keystroke(&Keystroke::parse("ctrl-c").unwrap()),
        None,
        "退出入口不能抢占PTY的Ctrl+C中断"
    );
}
