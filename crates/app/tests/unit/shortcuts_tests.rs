use super::*;

/// Cards 快速创建默认键位：六类可运行卡片必须按产品顺序占用 Cmd+1 到 Cmd+6。
///
/// 数据构造（含关键数值的推导过程）：
///   card shortcut count = 6（Terminal + Claude + Codex + Cursor + OpenCode + Grok）
///   first slot          = 1（Terminal）
///   last slot           = 6（Grok）
///   expected range      = Cmd+1..Cmd+6（6 - 1 + 1 = 6 个连续键位）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用空偏好构造 effective_shortcuts，确保没有用户 override 干扰。
///   2. 逐一读取六类卡片创建命令的 current_keystroke。
///   3. 同时检查 Terminal 与 Cursor 不再占用旧的 Cmd+N / Cmd+Shift+C。
///
/// 预期结果：
///   - 正断言：Terminal、Claude、Codex、Cursor、OpenCode、Grok 依次为 Cmd+1 到 Cmd+6。
///   - 负断言：Terminal 不再是 Cmd+N，Cursor 不再是 Cmd+Shift+C。
#[test]
fn test_card_creation_defaults_use_cmd_1_through_cmd_6_in_product_order() {
    let effective = effective_shortcuts(&ShortcutPreferences::default());

    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "Terminal 默认应使用 Cmd+1，作为第一类可运行卡片"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewClaudeAgent),
        Some("cmd-2"),
        "Claude 默认应使用 Cmd+2，紧随 Terminal"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewCodexAgent),
        Some("cmd-3"),
        "Codex 默认应使用 Cmd+3，不能与 Claude 对调"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewCursorAgent),
        Some("cmd-4"),
        "Cursor 默认应使用 Cmd+4，不能继续占用复制相关组合键"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewOpenCodeAgent),
        Some("cmd-5"),
        "OpenCode 默认应使用 Cmd+5"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewGrokAgent),
        Some("cmd-6"),
        "Grok 默认应使用 Cmd+6，补齐连续的六个创建键位"
    );
    assert_ne!(
        shortcut_for(&effective, ShortcutCommandId::NewTerminal),
        Some("cmd-n"),
        "Terminal 切换到 Cmd+1 后不应继续报告旧默认 Cmd+N"
    );
    assert_ne!(
        shortcut_for(&effective, ShortcutCommandId::NewCursorAgent),
        Some("cmd-shift-c"),
        "Cursor 切换到 Cmd+4 后不应继续报告旧默认 Cmd+Shift+C"
    );
}

/// 快捷键默认注册表：当前 app 命令应全部从 registry 暴露，Settings 入口采用 macOS 习惯的 Cmd+,。
///
/// 数据构造（含关键数值的推导过程）：
///   editable command count = 16
///     NewWindow / NewTerminal / NewBrowser / CopyCardContent / OpenProject / OpenPreviewFile /
///     NewClaudeAgent / NewCodexAgent / NewCursorAgent / NewOpenCodeAgent / NewGrokAgent / DeleteCard / ReflowLayout / ToggleMasterLayout /
///     ExitFocus / ToggleSettings
///   default assigned count = 15（除 NewBrowser 外均有默认键位）
///   read-only behavior count >= 5（Paste / Voice / Arrow navigation / Space pan / PTY input）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. default_shortcut_entries() 返回所有设置页可展示项
///   2. effective_shortcuts(default prefs) 计算默认有效快捷键
///   3. 查询关键命令的默认键位、可编辑性与只读行为
///
/// 预期结果：
///   - 正断言：NewWindow、NewTerminal、DeleteCard、ToggleSettings 等命令存在且默认键位正确
///   - 正断言：五种 Agent 均有连续的 Cmd+2 到 Cmd+6 默认创建快捷键
///   - 负断言：Cmd+V 不应作为可编辑命令出现，避免覆盖 paste 路由
#[test]
fn test_shortcut_registry_contains_defaults_and_read_only_behaviors() {
    let prefs = ShortcutPreferences::default();
    let effective = effective_shortcuts(&prefs);

    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewWindow),
        Some("cmd-shift-n"),
        "New Window 默认应使用 Cmd+Shift+N 打开独立启动页窗口"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "New Terminal 默认应使用 Cmd+1"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::DeleteCard),
        Some("cmd-d"),
        "Delete Card 默认应使用 Cmd+D"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::ToggleSettings),
        Some("cmd-,"),
        "Settings 默认入口应使用 macOS 习惯的 Cmd+,"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::ExitFocus),
        Some("cmd-escape"),
        "ExitFocus 默认应使用 cmd-escape，避免与 PTY Esc 输入冲突"
    );
    let delete_entry = entry_for(&effective, ShortcutCommandId::DeleteCard);
    assert_eq!(
        delete_entry.category, "Cards",
        "Delete Card 应归类在 Cards，Settings 才会出现在卡片命令分组"
    );
    assert_eq!(
        delete_entry.title, "Delete Card",
        "Delete Card 应使用明确标题，避免和复制/关闭等命令混淆"
    );
    assert!(
        delete_entry.editable,
        "Delete Card 应允许用户在 Settings 中改键"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewClaudeAgent),
        Some("cmd-2"),
        "New Claude Agent 默认应使用 Cmd+2，且可在 Settings 中重新录制"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewCodexAgent),
        Some("cmd-3"),
        "New Codex Agent 默认应使用 Cmd+3，且可在 Settings 中重新录制"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewCursorAgent),
        Some("cmd-4"),
        "New Cursor Agent 默认应使用 Cmd+4，且可在 Settings 中重新录制"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewOpenCodeAgent),
        Some("cmd-5"),
        "New OpenCode Agent 默认应使用 Cmd+5，且可在 Settings 中重新录制"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewGrokAgent),
        Some("cmd-6"),
        "New Grok Agent 默认应使用 Cmd+6，且可在 Settings 中重新录制"
    );
    assert!(
        read_only_behaviors()
            .iter()
            .any(|behavior| behavior.keystroke == "cmd-v" && behavior.title == "Paste"),
        "Cmd+V 应作为只读 Paste 行为展示"
    );
    assert!(
        effective
            .iter()
            .all(|entry| entry.current_keystroke.as_deref() != Some("cmd-v")),
        "Cmd+V 不应成为可编辑命令默认值，否则会覆盖 paste 路由"
    );
    assert_ne!(
        shortcut_for(&effective, ShortcutCommandId::NewCursorAgent),
        Some("cmd-c"),
        "Cursor 创建快捷键不能占用 Cmd+C，否则会破坏复制选区"
    );
}

/// 快捷键偏好覆盖：用户存储的单项 override 应覆盖默认键位，未覆盖项保持默认。
///
/// 数据构造（含关键数值的推导过程）：
///   override count = 1
///   target = NewTerminal
///   old default = cmd-1
///   new value = cmd-t
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 ShortcutPreferences，并设置 NewTerminal -> cmd-t
///   2. 调用 effective_shortcuts
///   3. 对比 NewTerminal 与 OpenProject 的有效值
///
/// 预期结果：
///   - 正断言：NewTerminal 有效键位变为 cmd-t
///   - 正断言：OpenProject 仍为 cmd-o
///   - 负断言：NewTerminal 不应继续显示旧默认 cmd-1
#[test]
fn test_shortcut_preferences_override_one_command_without_touching_others() {
    let mut prefs = ShortcutPreferences::default();
    prefs.set_override(ShortcutCommandId::NewTerminal, "cmd-t".to_string());

    let effective = effective_shortcuts(&prefs);

    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::NewTerminal),
        Some("cmd-t"),
        "用户覆盖后 NewTerminal 应使用 cmd-t"
    );
    assert_eq!(
        shortcut_for(&effective, ShortcutCommandId::OpenProject),
        Some("cmd-o"),
        "未覆盖的 OpenProject 应继续使用默认 cmd-o"
    );
    assert_ne!(
        shortcut_for(&effective, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "覆盖生效后 NewTerminal 不应继续显示默认 cmd-1"
    );
}

/// 快捷键持久化恢复：无效 JSON 或无效 keystroke 不应污染有效快捷键。
///
/// 数据构造（含关键数值的推导过程）：
///   invalid json = "{"
///   invalid keystroke override = "cmd-shift"（只有修饰键，GPUI parser 不接受）
///   fallback default for NewTerminal = cmd-1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ShortcutPreferences::from_json("{") → default
///   2. 构造包含 invalid keystroke 的 JSON 并反序列化
///   3. effective_shortcuts 忽略无效 override
///
/// 预期结果：
///   - 正断言：坏 JSON 回退 default
///   - 正断言：坏 keystroke 被忽略，NewTerminal 仍为 cmd-1
///   - 负断言：NewTerminal 不应显示无法解析的 cmd-shift
#[test]
fn test_shortcut_preferences_ignore_invalid_json_and_invalid_keystrokes() {
    let bad_json = ShortcutPreferences::from_json("{");
    let bad_keystroke = ShortcutPreferences::from_json(
        r#"{"overrides":[{"command":"new_terminal","keystroke":"cmd-shift"}]}"#,
    );

    let bad_json_effective = effective_shortcuts(&bad_json);
    let bad_keystroke_effective = effective_shortcuts(&bad_keystroke);

    assert_eq!(
        shortcut_for(&bad_json_effective, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "无法解析的 JSON 应回退默认 NewTerminal=cmd-1"
    );
    assert_eq!(
        shortcut_for(&bad_keystroke_effective, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "无法解析的 keystroke override 应被忽略并回退默认"
    );
    assert_ne!(
        shortcut_for(&bad_keystroke_effective, ShortcutCommandId::NewTerminal),
        Some("cmd-shift"),
        "无效 keystroke 不应进入有效快捷键"
    );
}

/// 快捷键冲突校验：重复可编辑命令与保留输入行为应被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   duplicate candidate = cmd-o（OpenProject 默认已占用）
///   duplicate card candidate = cmd-d（DeleteCard 默认已占用）
///   reserved candidates = cmd-v / cmd-e / up / space
///   bare printable = a
///   valid candidate = cmd-t
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以默认 effective shortcuts 调用 validate_shortcut_change
///   2. 分别检查重复、保留、裸字符和有效候选
///
/// 预期结果：
///   - 正断言：cmd-t 可用于 NewTerminal
///   - 负断言：cmd-o / cmd-d / cmd-v / cmd-e / up / space / a 均不允许
#[test]
fn test_shortcut_validation_rejects_duplicates_reserved_inputs_and_bare_printables() {
    let effective = effective_shortcuts(&ShortcutPreferences::default());

    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "cmd-t", &effective).is_ok(),
        "cmd-t 未被占用，也不是保留输入，应允许分配给 NewTerminal"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "cmd-o", &effective).is_err(),
        "cmd-o 已被 OpenProject 占用，不应允许重复分配"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "cmd-d", &effective).is_err(),
        "cmd-d 已被 DeleteCard 占用，不应允许重复分配"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "cmd-v", &effective).is_err(),
        "cmd-v 是 paste 保留输入，不应允许覆盖"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "cmd-e", &effective).is_err(),
        "cmd-e 是语音交互保留输入，不应允许覆盖"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "up", &effective).is_err(),
        "裸方向键属于画布导航，不应允许覆盖"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "space", &effective).is_err(),
        "裸 Space 属于画布 pan，不应允许覆盖"
    );
    assert!(
        validate_shortcut_change(ShortcutCommandId::NewTerminal, "a", &effective).is_err(),
        "裸可打印字符会干扰终端输入，不应允许作为 app 命令快捷键"
    );
}

/// 快捷键恢复默认：单项恢复只删除目标 override，全部恢复清空所有 override。
///
/// 数据构造（含关键数值的推导过程）：
///   override count = 2
///   NewTerminal: cmd-t
///   OpenProject: cmd-p
///   after single restore count = 1
///   after restore all count = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 设置两个 override
///   2. restore_default(NewTerminal)
///   3. restore_all_defaults()
///
/// 预期结果：
///   - 正断言：单项恢复后 NewTerminal 回到 cmd-1，OpenProject 保持 cmd-p
///   - 正断言：全部恢复后 OpenProject 回到 cmd-o
///   - 负断言：单项恢复不应误删 OpenProject override
#[test]
fn test_shortcut_restore_one_and_restore_all_defaults() {
    let mut prefs = ShortcutPreferences::default();
    prefs.set_override(ShortcutCommandId::NewTerminal, "cmd-t".to_string());
    prefs.set_override(ShortcutCommandId::OpenProject, "cmd-p".to_string());

    prefs.restore_default(ShortcutCommandId::NewTerminal);
    let after_one = effective_shortcuts(&prefs);

    assert_eq!(
        shortcut_for(&after_one, ShortcutCommandId::NewTerminal),
        Some("cmd-1"),
        "单项恢复 NewTerminal 后应回到默认 cmd-1"
    );
    assert_eq!(
        shortcut_for(&after_one, ShortcutCommandId::OpenProject),
        Some("cmd-p"),
        "单项恢复不应误删 OpenProject 的用户 override"
    );
    assert_ne!(
        shortcut_for(&after_one, ShortcutCommandId::OpenProject),
        Some("cmd-o"),
        "单项恢复 NewTerminal 后 OpenProject 不应提前回到默认值"
    );

    prefs.restore_all_defaults();
    let after_all = effective_shortcuts(&prefs);

    assert_eq!(
        shortcut_for(&after_all, ShortcutCommandId::OpenProject),
        Some("cmd-o"),
        "全部恢复后 OpenProject 应回到默认 cmd-o"
    );
}

fn shortcut_for(entries: &[EffectiveShortcutEntry], command: ShortcutCommandId) -> Option<&str> {
    entries
        .iter()
        .find(|entry| entry.command == command)
        .and_then(|entry| entry.current_keystroke.as_deref())
}

fn entry_for(
    entries: &[EffectiveShortcutEntry],
    command: ShortcutCommandId,
) -> &EffectiveShortcutEntry {
    entries
        .iter()
        .find(|entry| entry.command == command)
        .expect("测试命令应存在于 effective shortcuts")
}
