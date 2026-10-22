use super::*;

/// 只有单独按住 ⌃ 或 ⌃⇧ 且有待办想法时才显示提示；带 ⌘/⌥ 的组合与没有待办时一律不显示。
///
/// 数据构造：枚举修饰键组合与待办数量。
/// 预期结果：
///   - 正断言：⌃ → 卡片序号；⌃⇧ → 新建类型提示。
///   - 负断言：无修饰键、⌃⌘、⌃⌥、仅 ⇧ 或没有待办都不显示。
#[test]
fn badges_follow_exact_modifier_combinations() {
    assert_eq!(badge_mode(true, false, false, false, 2), BadgeMode::Cards);
    assert_eq!(badge_mode(true, true, false, false, 2), BadgeMode::NewAgent);
    assert_eq!(
        badge_mode(false, false, false, false, 2),
        BadgeMode::Hidden,
        "无修饰键"
    );
    assert_eq!(
        badge_mode(true, false, false, true, 2),
        BadgeMode::Hidden,
        "⌃⌘ 不是想法派发"
    );
    assert_eq!(
        badge_mode(true, false, true, false, 2),
        BadgeMode::Hidden,
        "⌃⌥ 不是想法派发"
    );
    assert_eq!(
        badge_mode(false, true, false, false, 2),
        BadgeMode::Hidden,
        "仅 ⇧"
    );
    assert_eq!(
        badge_mode(true, false, false, false, 0),
        BadgeMode::Hidden,
        "没有待办想法"
    );
}

/// 序号只给前 9 张卡片（⌃1…9 的范围），第 10 张起不显示。
///
/// 数据构造：0、3、11 张卡片。
/// 预期结果：长度与卡片数一致；前 9 张为 1…9，其后为 None。
#[test]
fn card_numbers_stop_at_nine() {
    assert!(card_numbers(0).is_empty());
    assert_eq!(card_numbers(3), [Some(1), Some(2), Some(3)]);
    let many = card_numbers(11);
    assert_eq!(many.len(), 11);
    assert_eq!(many[8], Some(9));
    assert_eq!(&many[9..], [None, None]);
}

/// 新建类型提示列出默认键与 ⌃⇧2…9 对应的类型，数字与 ⌘2…9 一致。
///
/// 预期结果：包含 `⌃0` 默认项以及 2→Claude、3→Codex、9→Hermes。
#[test]
fn new_agent_hint_lists_digit_to_profile() {
    let hint = new_agent_hint(norma::protocol::AgentProfile::Codex);
    assert!(hint.contains("⌃0 Codex"), "{hint}");
    assert!(
        hint.contains("2 Claude") && hint.contains("3 Codex") && hint.contains("9 Hermes"),
        "{hint}"
    );
}
