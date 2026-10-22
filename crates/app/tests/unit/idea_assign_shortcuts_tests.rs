use super::*;
use crate::shortcuts::{default_shortcut_entries, effective_shortcuts, ShortcutPreferences};
use gpui::Keystroke;

/// 数字槽位与 `⌘2…9` 的创建顺序一致，`0` 与未绑定槽位交给默认类型。
///
/// 数据构造：枚举槽位 0..=9。
/// 预期结果：2..=9 依次是 Claude、Codex、Cursor、OpenCode、Grok、Kimi、Pi、Hermes；0 与 1 为 None。
#[test]
fn new_agent_slots_follow_cmd_creation_order() {
    let expected = [
        (2, AgentProfile::Claude),
        (3, AgentProfile::Codex),
        (4, AgentProfile::Cursor),
        (5, AgentProfile::OpenCode),
        (6, AgentProfile::Grok),
        (7, AgentProfile::Kimi),
        (8, AgentProfile::Pi),
        (9, AgentProfile::Hermes),
    ];
    for (slot, profile) in expected {
        assert_eq!(new_agent_profile(slot), Some(profile), "槽位 {slot} 的类型");
    }
    assert_eq!(new_agent_profile(0), None, "0 使用默认类型");
    assert_eq!(new_agent_profile(1), None, "1 没有绑定类型");
}

/// 默认键位下 18 条命令都能被各自的按键唯一识别，且互不冲突、不抢占已有命令。
///
/// 数据构造：用默认偏好生成有效快捷键表。
/// 执行过程：逐个解析 `ctrl-N`、`ctrl-0`、`ctrl-shift-N`，按 `command_for_keystroke` 同样的 unparse 规则查表。
/// 预期结果：
///   - 正断言：`ctrl-3` 对应 AssignIdeaToCard(3)；`ctrl-shift-3` 对应 AssignIdeaToNewAgent(3)；`ctrl-0` 对应 AssignIdeaToNewAgent(0)。
///   - 负断言：整张表没有重复按键；`cmd-3`（新建 Codex）仍是原命令。
#[test]
fn default_idea_assign_keystrokes_are_unique_and_do_not_shadow_existing() {
    let effective = effective_shortcuts(&ShortcutPreferences::default());
    let lookup = |source: &str| {
        let key = Keystroke::parse(source).expect("应能解析").unparse();
        effective
            .iter()
            .find(|entry| entry.current_keystroke.as_deref() == Some(key.as_str()))
            .map(|entry| entry.command)
    };
    assert_eq!(
        lookup("ctrl-3"),
        Some(ShortcutCommandId::AssignIdeaToCard(3)),
        "ctrl-3 派给第 3 张卡片"
    );
    assert_eq!(
        lookup("ctrl-shift-3"),
        Some(ShortcutCommandId::AssignIdeaToNewAgent(3)),
        "ctrl-shift-3 新建 Codex 并派发"
    );
    assert_eq!(
        lookup("ctrl-0"),
        Some(ShortcutCommandId::AssignIdeaToNewAgent(0)),
        "ctrl-0 新建默认类型"
    );
    assert_eq!(
        lookup("cmd-3"),
        Some(ShortcutCommandId::NewCodexAgent),
        "cmd-3 仍是新建 Codex"
    );
    let mut keys: Vec<_> = effective
        .iter()
        .filter_map(|e| e.current_keystroke.clone())
        .collect();
    let total = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), total, "默认快捷键不应有重复");
}

/// 18 条命令都有 Settings 行 ID 与恢复按钮 ID，且全表不重复。
///
/// 预期结果：目录里每个想法派发命令 settings_ids 为 Some；越界槽位和无关命令为 None；ID 全局唯一。
#[test]
fn every_idea_assign_command_has_unique_settings_ids() {
    let mut seen = Vec::new();
    for entry in default_shortcut_entries() {
        if matches!(
            entry.command,
            ShortcutCommandId::AssignIdeaToCard(_) | ShortcutCommandId::AssignIdeaToNewAgent(_)
        ) {
            let (row, restore) = settings_ids(entry.command).expect("想法派发命令应有 Settings ID");
            seen.push(row);
            seen.push(restore);
        }
    }
    assert_eq!(seen.len(), 38, "19 条命令各有录制行与恢复按钮");
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "Settings ID 不应重复");
    assert_eq!(
        settings_ids(ShortcutCommandId::AssignIdeaToCard(0)),
        None,
        "槽位 0 不属于卡片命令"
    );
    assert_eq!(
        settings_ids(ShortcutCommandId::NewTerminal),
        None,
        "无关命令没有想法派发 ID"
    );
}

/// `Tab` 轮换默认类型：按 `⌘2…9` 顺序前进并首尾相接，不在轮换里的类型回到 Claude。
///
/// 数据构造：从 Claude、Codex、Hermes（末位）、ZCode（不在轮换）出发各取一次。
/// 预期结果：Claude→Codex；Codex→Cursor；Hermes→Claude（环回）；ZCode→Claude。
#[test]
fn next_default_profile_cycles_in_cmd_order() {
    assert_eq!(
        next_default_profile(AgentProfile::Claude),
        AgentProfile::Codex,
        "Claude 后是 Codex"
    );
    assert_eq!(
        next_default_profile(AgentProfile::Codex),
        AgentProfile::Cursor,
        "Codex 后是 Cursor"
    );
    assert_eq!(
        next_default_profile(AgentProfile::Hermes),
        AgentProfile::Claude,
        "末位应环回"
    );
    assert_eq!(
        next_default_profile(AgentProfile::ZCode),
        AgentProfile::Claude,
        "不在轮换里回到第一个"
    );
}

/// 选择器列出全部 10 种类型：数字键 2…9 的类型在前（顺序与 `⌘2…9` 一致），ZCode、InfCode 在后；每种恰好一次。
#[test]
fn picker_profiles_cover_every_type_once_in_digit_order() {
    let all = picker_profiles();
    assert_eq!(all.len(), 10);
    assert_eq!(
        &all[..8],
        &(2..=9).filter_map(new_agent_profile).collect::<Vec<_>>()[..]
    );
    assert_eq!(&all[8..], [AgentProfile::ZCode, AgentProfile::InfCode]);
    let unique: std::collections::HashSet<String> = all.iter().map(|p| format!("{p:?}")).collect();
    assert_eq!(unique.len(), 10);
}

/// 选择器里按数字键直达：2…9 对应类型，其他数字无效。
#[test]
fn picker_digit_selects_profile() {
    assert_eq!(picker_index_for_digit('2'), Some(0));
    assert_eq!(picker_index_for_digit('9'), Some(7));
    assert_eq!(picker_index_for_digit('1'), None);
    assert_eq!(picker_index_for_digit('x'), None);
}

/// 默认新建类型的存储键值：每种类型往返一致；未知、空值和大小写不符（键是小写）都视为未保存。
///
/// 预期结果：全部 10 种类型 key → 解析回原类型；"" / "codex" / "Unknown" 为 None。
#[test]
fn default_profile_key_round_trips() {
    for profile in picker_profiles() {
        assert_eq!(
            profile_from_key(profile_key(profile)),
            Some(profile),
            "{profile:?}"
        );
    }
    assert_eq!(profile_from_key(""), None);
    assert_eq!(profile_from_key("Codex"), None);
    assert_eq!(profile_from_key("Unknown"), None);
}
