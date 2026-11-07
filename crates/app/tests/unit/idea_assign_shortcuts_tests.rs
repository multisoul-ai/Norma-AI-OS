use super::*;
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

/// macOS 上 GPUI 对 shift+数字上报的是“上档字符且清掉 shift 位”（⌃⇧3 → `ctrl-#`）：
/// 录制时必须还原成规范的 `ctrl-shift-N`，其他修饰组合保持原样。
///
/// 数据构造：真实形态 `ctrl-#`，以及带 ⌘ 的 `ctrl-cmd-#`。
/// 预期结果：`ctrl-#` → `ctrl-shift-3`；`ctrl-cmd-#` 不被改写。
#[test]
fn ctrl_shift_digit_is_recorded_in_canonical_form() {
    let recorded = canonical_source(&Keystroke::parse("ctrl-#").expect("有效按键"));
    assert_eq!(
        recorded, "ctrl-shift-3",
        "仅带 ⌃ 的上档字符应还原成 ctrl-shift-3"
    );
    let untouched = canonical_source(&Keystroke::parse("ctrl-cmd-#").expect("有效按键"));
    assert_eq!(untouched, "ctrl-cmd-#", "带 ⌘ 的组合不得被改写");
}
