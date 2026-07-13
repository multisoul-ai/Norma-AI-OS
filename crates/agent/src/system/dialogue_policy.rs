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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DialogueRoutingDecision {
    ModelOnly,
    RequireCardTaskFallback,
}

pub(super) fn route_user_turn(
    text: &str,
    roster: &[crate::AgentRosterEntry],
) -> DialogueRoutingDecision {
    let normalized = strip_execution_modifiers(text.trim().to_lowercase());
    if normalized.is_empty()
        || contains_any(&normalized, EXPLANATION_MARKERS)
        || contains_any(&normalized, EXPLANATION_NOUN_PHRASES)
        || contains_any(&normalized, CREATE_NEGATIONS)
    {
        return DialogueRoutingDecision::ModelOnly;
    }
    if contains_any(&normalized, CREATE_ACTIONS) && contains_any(&normalized, CONCRETE_CARD_TARGETS)
    {
        DialogueRoutingDecision::RequireCardTaskFallback
    } else if has_explicit_agent_address(text, roster) {
        DialogueRoutingDecision::RequireCardTaskFallback
    } else {
        DialogueRoutingDecision::ModelOnly
    }
}

fn has_explicit_agent_address(text: &str, roster: &[crate::AgentRosterEntry]) -> bool {
    let trimmed = text.trim();
    roster.iter().any(|agent| {
        address_keys(agent).into_iter().any(|key| {
            starts_with_punctuated_name(trimmed, &key)
                || ["让", "请", "交给", "打开"].iter().any(|prefix| {
                    trimmed.strip_prefix(prefix).is_some_and(|remainder| {
                        starts_with_named_instruction(
                            remainder.trim_start(),
                            &key,
                            *prefix == "打开",
                        )
                    })
                })
        })
    })
}

fn address_keys(agent: &crate::AgentRosterEntry) -> Vec<String> {
    let mut keys = vec![agent.name.clone()];
    if let Some(alias) = agent.speech_alias.as_ref() {
        keys.push(alias.clone());
    } else if let Some(alias) = crate::builtin_agent_names()
        .iter()
        .find(|builtin| builtin.name().eq_ignore_ascii_case(&agent.name))
        .map(|builtin| builtin.speech_alias())
    {
        keys.push(alias.to_string());
    }
    keys
}

fn starts_with_punctuated_name(text: &str, name: &str) -> bool {
    strip_name_prefix(text, name).is_some_and(|remainder| {
        let remainder = remainder.trim_start();
        let Some(punctuation) = remainder.chars().next() else {
            return false;
        };
        matches!(punctuation, ',' | '，' | ':' | '：')
            && !remainder[punctuation.len_utf8()..].trim().is_empty()
    })
}

fn starts_with_named_instruction(text: &str, name: &str, allow_empty: bool) -> bool {
    strip_name_prefix(text, name).is_some_and(|remainder| {
        let remainder = remainder.trim_start_matches(|character: char| {
            character.is_whitespace() || matches!(character, ',' | '，' | ':' | '：')
        });
        !remainder.is_empty() || allow_empty
    })
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
