pub(super) fn has_explicit_delete_intent(
    input: &str,
    node_id: u64,
    contextual_delete_targets: &[u64],
) -> bool {
    let normalized = input.to_lowercase();
    if has_negated_delete_intent(&normalized) {
        return false;
    }
    let has_delete_verb = has_delete_verb(&normalized);
    let has_exact_target = has_explicit_node_reference(&normalized, node_id);
    let has_contextual_target = [
        "当前卡片",
        "这张卡片",
        "选中卡片",
        "当前选中的卡片",
        "focused card",
        "selected card",
        "this card",
    ]
    .iter()
    .any(|target| normalized.contains(target));
    let contextual_target_matches =
        has_contextual_target && contextual_delete_targets.contains(&node_id);
    has_delete_verb && (has_exact_target || contextual_target_matches)
}

fn has_explicit_node_reference(normalized: &str, node_id: u64) -> bool {
    let compact = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    [
        format!("#{node_id}"),
        format!("node_id={node_id}"),
        format!("节点#{node_id}"),
        format!("节点{node_id}"),
        format!("卡片编号#{node_id}"),
        format!("卡片编号{node_id}"),
    ]
    .iter()
    .any(|marker| {
        compact.match_indices(marker).any(|(start, _)| {
            let suffix = &compact[start + marker.len()..];
            suffix
                .chars()
                .next()
                .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
        })
    })
}

pub(super) fn has_explicit_position_delete_intent(input: &str, position: u64) -> bool {
    let normalized = input.to_lowercase();
    if has_negated_delete_intent(&normalized) || !has_delete_verb(&normalized) {
        return false;
    }
    let compact = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let numeric_targets = [
        format!("第{position}个"),
        format!("第{position}张"),
        format!("位置{position}"),
    ];
    if numeric_targets
        .iter()
        .any(|target| compact.contains(target))
    {
        return true;
    }
    let chinese_number = match position {
        1 => Some("一"),
        2 => Some("二"),
        3 => Some("三"),
        4 => Some("四"),
        5 => Some("五"),
        6 => Some("六"),
        7 => Some("七"),
        8 => Some("八"),
        9 => Some("九"),
        10 => Some("十"),
        _ => None,
    };
    chinese_number.is_some_and(|number| {
        compact.contains(&format!("第{number}个")) || compact.contains(&format!("第{number}张"))
    })
}

pub(super) fn has_explicit_agent_delete_intent(input: &str, agent: &str) -> bool {
    let normalized = input.to_lowercase();
    if has_negated_delete_intent(&normalized) || !has_delete_verb(&normalized) {
        return false;
    }
    let compact = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let target = agent
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    if target.is_empty() {
        return false;
    }
    [
        "删除", "删掉", "移除", "关闭", "关掉", "delete", "remove", "close",
    ]
    .iter()
    .any(|verb| {
        let marker = format!("{verb}{target}");
        compact.match_indices(&marker).any(|(start, _)| {
            let suffix = &compact[start + marker.len()..];
            exact_agent_target_suffix(suffix)
        })
    })
}

fn exact_agent_target_suffix(suffix: &str) -> bool {
    if suffix.is_empty() || starts_with_target_boundary(suffix) {
        return true;
    }
    [
        "卡片",
        "的卡片",
        "这个agent卡片",
        "的agent卡片",
        "agent卡片",
        "card",
        "agentcard",
        "'scard",
        "'sagentcard",
        "吧",
        "please",
    ]
    .iter()
    .any(|qualifier| {
        suffix
            .strip_prefix(qualifier)
            .is_some_and(|remainder| remainder.is_empty() || starts_with_target_boundary(remainder))
    })
}

fn starts_with_target_boundary(value: &str) -> bool {
    value.chars().next().is_some_and(|character| {
        matches!(
            character,
            ',' | '，' | '。' | '!' | '！' | '.' | '?' | '？' | ';' | '；'
        )
    })
}

fn has_negated_delete_intent(normalized: &str) -> bool {
    [
        "不要删除",
        "不用删除",
        "无需删除",
        "别删除",
        "不删除",
        "不要删",
        "不用删",
        "别删",
        "不要移除",
        "不用移除",
        "无需移除",
        "别移除",
        "不要关闭",
        "不用关闭",
        "无需关闭",
        "别关闭",
        "不要关掉",
        "不用关掉",
        "无需关掉",
        "别关掉",
        "do not delete",
        "do not remove",
        "do not close",
        "don't delete",
        "don't remove",
        "don't close",
        "dont delete",
        "dont remove",
        "dont close",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn has_delete_verb(normalized: &str) -> bool {
    [
        "删除", "删掉", "移除", "关闭", "关掉", "delete", "remove", "close",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}
