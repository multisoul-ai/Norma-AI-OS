const EXPLANATION_MARKERS: &[&str] = &[
    "如何",
    "怎么",
    "怎样",
    "为什么",
    "教程",
    "说明",
    "解释",
    "是什么",
    "有哪些风险",
    "可行性",
    "how to",
    "what is",
    "explain",
];
const EXPLANATION_NOUN_PHRASES: &[&str] = &["的方案", "的方法", "的步骤"];
const CREATE_NEGATIONS: &[&str] = &[
    "不要创建",
    "不用创建",
    "别创建",
    "无需创建",
    "don't create",
    "do not create",
];
const CREATE_ACTIONS: &[&str] = &[
    "创建", "新建", "打开", "添加", "生成", "create", "open", "add", "spawn",
];
const CONCRETE_CARD_TARGETS: &[&str] = &[
    "终端",
    "terminal",
    "codex",
    "claude",
    "opencode",
    "agent 卡片",
    "agent卡片",
    "agent card",
    "智能体卡片",
];
const AGENT_TASK_ACTIONS: &[&str] = &[
    "搜索",
    "查找",
    "分析",
    "检查",
    "审查",
    "运行",
    "测试",
    "修复",
    "实现",
    "开发",
    "编写",
    "写",
    "重构",
    "调查",
    "整理",
    "更新",
    "创建",
    "执行",
    "查看",
    "阅读",
    "search",
    "find",
    "analyze",
    "analyse",
    "check",
    "review",
    "run",
    "test",
    "fix",
    "implement",
    "build",
    "write",
    "refactor",
    "update",
    "create",
];
const DIRECT_ADDRESS_MODIFIERS: &[&str] = &[
    "你们", "你", "都", "一起", "去", "帮我", "帮忙", "请", "麻烦",
];
const AGENT_GROUP_SEPARATORS: &[&str] = &["和", "与", "、", ",", "，", "and"];
const NON_COMMAND_PREFIXES: &[&str] = &["看起来", "is ", "was ", "seems "];
const NON_COMMAND_ACTION_REMAINDERS: &[&str] = &[
    "的",
    "得",
    "能力",
    "结束",
    "完成",
    "状态",
    "进度",
    "怎么样",
    "如何",
    "了吗",
    "了没",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DialogueRoutingDecision {
    ModelOnly,
    RequireCardTaskFallback { delegation_targets: Vec<String> },
}

impl DialogueRoutingDecision {
    pub(super) fn requires_card_task_fallback(&self) -> bool {
        matches!(self, Self::RequireCardTaskFallback { .. })
    }

    pub(super) fn delegation_targets(&self) -> Option<&[String]> {
        match self {
            Self::ModelOnly => None,
            Self::RequireCardTaskFallback { delegation_targets }
                if delegation_targets.is_empty() =>
            {
                None
            }
            Self::RequireCardTaskFallback { delegation_targets } => Some(delegation_targets),
        }
    }
}

pub(super) fn route_user_turn(
    text: &str,
    roster: &[crate::AgentRosterEntry],
) -> DialogueRoutingDecision {
    let normalized = strip_execution_modifiers(text.trim().to_lowercase());
    if normalized.is_empty() || contains_any(&normalized, CREATE_NEGATIONS) {
        return DialogueRoutingDecision::ModelOnly;
    }
    let delegation_targets = explicit_agent_targets(text, roster);
    if !delegation_targets.is_empty() {
        return DialogueRoutingDecision::RequireCardTaskFallback { delegation_targets };
    }
    if contains_any(&normalized, EXPLANATION_MARKERS)
        || contains_any(&normalized, EXPLANATION_NOUN_PHRASES)
    {
        return DialogueRoutingDecision::ModelOnly;
    }
    if contains_any(&normalized, CREATE_ACTIONS) && contains_any(&normalized, CONCRETE_CARD_TARGETS)
    {
        return DialogueRoutingDecision::RequireCardTaskFallback {
            delegation_targets: Vec::new(),
        };
    }
    DialogueRoutingDecision::ModelOnly
}

fn explicit_agent_targets(text: &str, roster: &[crate::AgentRosterEntry]) -> Vec<String> {
    let trimmed = text.trim();
    if let Some(targets) = agent_group_targets(trimmed, roster) {
        return targets;
    }
    let keys = sorted_address_keys(roster);
    let mut targets = direct_target(trimmed, &keys)
        .into_iter()
        .collect::<Vec<_>>();
    for clause in trimmed.split([',', '，', ';', '；']).skip(1) {
        if let Some(target) = unpunctuated_target(clause.trim_start(), &keys) {
            push_unique(&mut targets, target);
        }
    }
    targets
}

fn sorted_address_keys(roster: &[crate::AgentRosterEntry]) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    for agent in roster {
        keys.push((agent.name.clone(), agent.name.clone()));
        if let Some(alias) = agent.speech_alias.as_ref() {
            keys.push((alias.clone(), agent.name.clone()));
        }
    }
    keys.sort_by_key(|(key, _)| std::cmp::Reverse(key.chars().count()));
    keys
}

fn direct_target(text: &str, keys: &[(String, String)]) -> Option<String> {
    keys.iter().find_map(|(key, canonical)| {
        (starts_with_punctuated_instruction(text, key)
            || starts_with_unpunctuated_instruction(text, key)
            || ["让", "请", "交给", "打开"].iter().any(|prefix| {
                text.strip_prefix(prefix).is_some_and(|remainder| {
                    starts_with_named_instruction(remainder.trim_start(), key, *prefix == "打开")
                })
            }))
        .then(|| canonical.clone())
    })
}

fn unpunctuated_target(text: &str, keys: &[(String, String)]) -> Option<String> {
    keys.iter().find_map(|(key, canonical)| {
        starts_with_unpunctuated_instruction(text, key).then(|| canonical.clone())
    })
}

fn starts_with_punctuated_instruction(text: &str, name: &str) -> bool {
    strip_name_prefix(text, name)
        .and_then(|remainder| {
            let remainder = remainder.trim_start();
            let punctuation = remainder.chars().next()?;
            matches!(punctuation, ',' | '，' | ':' | '：')
                .then(|| &remainder[punctuation.len_utf8()..])
        })
        .map(str::trim_start)
        .map(strip_direct_address_modifiers)
        .is_some_and(starts_with_task_action)
}

fn starts_with_named_instruction(text: &str, name: &str, allow_empty: bool) -> bool {
    strip_name_prefix(text, name).is_some_and(|remainder| {
        let remainder = remainder.trim_start_matches(|character: char| {
            character.is_whitespace() || matches!(character, ',' | '，' | ':' | '：')
        });
        !remainder.is_empty() || allow_empty
    })
}

fn starts_with_unpunctuated_instruction(text: &str, name: &str) -> bool {
    strip_name_prefix(text, name)
        .map(str::trim_start)
        .map(strip_direct_address_modifiers)
        .is_some_and(starts_with_task_action)
}

fn agent_group_targets(text: &str, roster: &[crate::AgentRosterEntry]) -> Option<Vec<String>> {
    let keys = sorted_address_keys(roster);
    let Some((first_target, mut remainder)) = strip_any_name_prefix(text, &keys) else {
        return None;
    };
    let mut targets = vec![first_target.to_string()];
    loop {
        remainder = remainder.trim_start();
        let Some(after_separator) = strip_any_prefix(remainder, AGENT_GROUP_SEPARATORS) else {
            break;
        };
        let Some((target, after_name)) = strip_any_name_prefix(after_separator.trim_start(), &keys)
        else {
            break;
        };
        push_unique(&mut targets, target.to_string());
        remainder = after_name;
    }
    if targets.len() < 2 {
        return None;
    }
    let instruction = remainder.trim_start_matches(|character: char| {
        character.is_whitespace() || matches!(character, ',' | '，' | ':' | '：')
    });
    starts_with_task_action(strip_direct_address_modifiers(instruction)).then_some(targets)
}

fn strip_any_name_prefix<'a, 'b>(
    text: &'a str,
    keys: &'b [(String, String)],
) -> Option<(&'b str, &'a str)> {
    keys.iter()
        .filter_map(|(key, canonical)| {
            strip_name_prefix(text, key)
                .map(|remainder| (key.chars().count(), canonical.as_str(), remainder))
        })
        .max_by_key(|(character_count, _, _)| *character_count)
        .map(|(_, canonical, remainder)| (canonical, remainder))
}

fn strip_any_prefix<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|prefix| {
        if prefix.is_ascii() {
            text.get(..prefix.len())
                .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))?;
            text.get(prefix.len()..)
        } else {
            text.strip_prefix(prefix)
        }
    })
}

fn strip_direct_address_modifiers(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let Some(remainder) = strip_any_prefix(trimmed, DIRECT_ADDRESS_MODIFIERS) else {
            return trimmed;
        };
        text = remainder;
    }
}

fn starts_with_task_action(text: &str) -> bool {
    if NON_COMMAND_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
    {
        return false;
    }
    AGENT_TASK_ACTIONS.iter().any(|action| {
        if action.is_ascii() {
            text.get(..action.len()).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case(action)
                    && !text[action.len()..]
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_alphabetic())
                    && !starts_with_non_command_remainder(&text[action.len()..])
            })
        } else {
            text.strip_prefix(action)
                .is_some_and(|remainder| !starts_with_non_command_remainder(remainder))
        }
    })
}

fn starts_with_non_command_remainder(remainder: &str) -> bool {
    NON_COMMAND_ACTION_REMAINDERS
        .iter()
        .any(|prefix| remainder.starts_with(prefix))
}

fn push_unique(targets: &mut Vec<String>, target: String) {
    if !targets.iter().any(|existing| existing == &target) {
        targets.push(target);
    }
}

fn strip_name_prefix<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    if name.is_ascii() {
        text.get(..name.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(name))?;
        let remainder = text.get(name.len()..)?;
        if remainder
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        {
            None
        } else {
            Some(remainder)
        }
    } else {
        text.strip_prefix(name)
    }
}

fn strip_execution_modifiers(mut text: String) -> String {
    for modifier in [
        "别解释",
        "不要解释",
        "不用解释",
        "无需解释",
        "don't explain",
        "do not explain",
    ] {
        text = text.replace(modifier, "");
    }
    text
}

fn contains_any(text: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| text.contains(candidate))
}
