use crate::{AgentRosterEntry, AgentTaskState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressedAgent {
    pub definition_id: i64,
    pub name: String,
    pub active: bool,
    pub task_state: AgentTaskState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectAddressIntent {
    /// Send message into each active agent session (session reply semantics).
    SessionReply {
        agents: Vec<AddressedAgent>,
        message: String,
    },
    /// Explicit new-task phrasing → assign_agent_task semantics.
    NewTask {
        agents: Vec<AddressedAgent>,
        message: String,
    },
    /// Inactive agent + context-dependent phrasing → fail with zero side effects.
    RejectContextDependent {
        agents: Vec<AddressedAgent>,
        message: String,
        reason: &'static str,
    },
    /// Self-contained request for inactive agent → may auto-activate as new task.
    ActivateAndAssign {
        agents: Vec<AddressedAgent>,
        message: String,
    },
    /// Status / summary questions or ordinary mentions — no side effects.
    NoSideEffect,
}

/// Pure direct-address router used before the model turn (SPEC §4.2 / §6.3).
pub fn parse_direct_address(input: &str, roster: &[AgentRosterEntry]) -> DirectAddressIntent {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DirectAddressIntent::NoSideEffect;
    }

    if let Some(intent) = parse_give_agent_new_task(trimmed, roster) {
        return intent;
    }

    let Some((agents, remainder)) = match_leading_agents(trimmed, roster) else {
        return DirectAddressIntent::NoSideEffect;
    };
    let message = strip_leading_separators(remainder);
    if message.is_empty() {
        return DirectAddressIntent::NoSideEffect;
    }

    if is_status_question(message) {
        return DirectAddressIntent::NoSideEffect;
    }
    if looks_like_ordinary_mention(message) {
        return DirectAddressIntent::NoSideEffect;
    }

    if has_explicit_new_task_phrase(message) {
        let cleaned = strip_new_task_prefix(message);
        return DirectAddressIntent::NewTask {
            agents,
            message: cleaned,
        };
    }

    if agents
        .iter()
        .any(|agent| agent.task_state == AgentTaskState::Ended)
    {
        return DirectAddressIntent::NewTask {
            agents,
            message: message.to_string(),
        };
    }

    let all_active = agents.iter().all(|agent| agent.active);
    let any_inactive = agents.iter().any(|agent| !agent.active);

    if all_active {
        return DirectAddressIntent::SessionReply {
            agents,
            message: message.to_string(),
        };
    }

    if any_inactive && is_context_dependent(message) {
        return DirectAddressIntent::RejectContextDependent {
            agents,
            message: message.to_string(),
            reason: "还没有可续接的卡片或会话",
        };
    }

    if any_inactive {
        return DirectAddressIntent::ActivateAndAssign {
            agents,
            message: message.to_string(),
        };
    }

    DirectAddressIntent::NoSideEffect
}

fn parse_give_agent_new_task(
    input: &str,
    roster: &[AgentRosterEntry],
) -> Option<DirectAddressIntent> {
    let rest = starts_with_ignore_ascii_case(input, "给")?.trim_start();
    let (agent, after_name) = match_one_leading_agent(rest, roster)?;
    let after_name = strip_leading_separators(after_name);
    if !(after_name.contains("新任务") || after_name.to_lowercase().contains("new task")) {
        return None;
    }
    let message = strip_new_task_prefix(after_name);
    if message.is_empty() {
        return None;
    }
    Some(DirectAddressIntent::NewTask {
        agents: vec![agent],
        message,
    })
}

fn match_leading_agents<'a>(
    input: &'a str,
    roster: &[AgentRosterEntry],
) -> Option<(Vec<AddressedAgent>, &'a str)> {
    let mut remaining = input;
    let mut matched = Vec::new();
    loop {
        let Some((agent, rest)) = match_one_leading_agent(remaining, roster) else {
            break;
        };
        if matched
            .iter()
            .any(|existing: &AddressedAgent| existing.definition_id == agent.definition_id)
        {
            break;
        }
        matched.push(agent);
        remaining = strip_leading_conjunction(rest);
        if matched.len() >= 8 {
            break;
        }
        // Continue only while the next token still looks like an address chain.
        if match_one_leading_agent(remaining, roster).is_none() {
            break;
        }
    }
    if matched.is_empty() {
        None
    } else {
        Some((matched, remaining))
    }
}

fn match_one_leading_agent<'a>(
    input: &'a str,
    roster: &[AgentRosterEntry],
) -> Option<(AddressedAgent, &'a str)> {
    let mut candidates = roster
        .iter()
        .filter(|entry| entry.enabled)
        .filter_map(|entry| {
            let alias = entry
                .speech_alias
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let name_hit = starts_with_ignore_ascii_case(input, &entry.name)
                .map(|rest| (entry, rest, entry.name.len()));
            let alias_hit = alias.and_then(|alias| {
                starts_with_ignore_ascii_case(input, alias).map(|rest| (entry, rest, alias.len()))
            });
            match (name_hit, alias_hit) {
                (Some(name), Some(alias)) => Some(if name.2 >= alias.2 { name } else { alias }),
                (Some(name), None) => Some(name),
                (None, Some(alias)) => Some(alias),
                (None, None) => None,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.2.cmp(&left.2));
    let (entry, rest, _) = candidates.into_iter().next()?;
    // Require a separator or end after the name so "Alanine" does not match "Alan".
    if !rest.is_empty() && !is_address_separator(rest.chars().next().unwrap_or('\0')) {
        return None;
    }
    Some((
        AddressedAgent {
            definition_id: entry.definition_id,
            name: entry.name.clone(),
            active: entry.active_node_id.is_some(),
            task_state: entry.task_state,
        },
        rest,
    ))
}

fn starts_with_ignore_ascii_case<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let input_chars = input.chars();
    let mut prefix_chars = prefix.chars();
    let mut consumed = 0usize;
    for input_ch in input_chars {
        let Some(prefix_ch) = prefix_chars.next() else {
            return Some(&input[consumed..]);
        };
        if !eq_ignore_case(input_ch, prefix_ch) {
            return None;
        }
        consumed += input_ch.len_utf8();
    }
    if prefix_chars.next().is_some() {
        None
    } else {
        Some(&input[consumed..])
    }
}

fn eq_ignore_case(left: char, right: char) -> bool {
    left.eq_ignore_ascii_case(&right) || left == right
}

fn is_address_separator(ch: char) -> bool {
    matches!(
        ch,
        ',' | '，'
            | ':'
            | '：'
            | ';'
            | '；'
            | ' '
            | '\t'
            | '、'
            | '!'
            | '！'
            | '?'
            | '？'
            | '.'
            | '。'
    ) || ch.is_whitespace()
}

fn strip_leading_separators(input: &str) -> &str {
    input
        .trim_start_matches(|ch: char| is_address_separator(ch) || matches!(ch, '请' | '帮'))
        .trim_start()
}

fn strip_leading_conjunction(input: &str) -> &str {
    let trimmed = input.trim_start_matches(is_address_separator).trim_start();
    for conjunction in ["和", "与", "以及", "还有", "and", "&"] {
        if let Some(rest) = starts_with_ignore_ascii_case(trimmed, conjunction) {
            return rest.trim_start_matches(is_address_separator).trim_start();
        }
    }
    trimmed
}

fn has_explicit_new_task_phrase(message: &str) -> bool {
    const PHRASES: &[&str] = &[
        "新任务",
        "另一个任务",
        "单独派发",
        "排一个任务",
        "给一个新任务",
        "new task",
        "another task",
    ];
    let lower = message.to_lowercase();
    PHRASES
        .iter()
        .any(|phrase| message.contains(phrase) || lower.contains(&phrase.to_lowercase()))
        || message.starts_with("给") && message.contains("一个新任务")
}

fn strip_new_task_prefix(message: &str) -> String {
    let text = message.trim().to_string();
    for prefix in [
        "给一个新任务：",
        "给一个新任务:",
        "一个新任务：",
        "一个新任务:",
        "新任务：",
        "新任务:",
        "new task:",
        "New task:",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.trim().to_string();
        }
    }
    // "给 Alan 一个新任务：检查支付模块" already removed agent; message may be
    // "一个新任务：检查支付模块" or "检查支付模块" after address parse.
    if let Some(idx) = text.find('：').or_else(|| text.find(':')) {
        let (head, tail) = text.split_at(idx);
        if head.contains("新任务") || head.to_lowercase().contains("new task") {
            return tail[1..].trim().to_string();
        }
    }
    text
}

fn is_context_dependent(message: &str) -> bool {
    const MARKERS: &[&str] = &[
        "继续",
        "接着",
        "刚才",
        "之前",
        "上面",
        "这个",
        "那个",
        "照此",
        "就这样",
        "同意",
        "不同意",
        "按 a",
        "按 b",
        "按a",
        "按b",
        "方案 a",
        "方案 b",
        "方案a",
        "方案b",
        "continue",
        "that",
        "this",
        "previous",
        "earlier",
        "above",
    ];
    let lower = message.to_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
        || lower.contains("按 a 方案")
        || lower.contains("按 b 方案")
        || message.contains("按 A")
        || message.contains("按 B")
        || message.contains("A 方案")
        || message.contains("B 方案")
}

fn is_status_question(message: &str) -> bool {
    let lower = message.to_lowercase();
    const MARKERS: &[&str] = &[
        "刚才建议",
        "建议什么",
        "建议了什么",
        "在等我",
        "等我决定",
        "谁在等",
        "最后是在问",
        "说了什么",
        "what did",
        "waiting for",
    ];
    MARKERS
        .iter()
        .any(|marker| message.contains(marker) || lower.contains(marker))
}

fn looks_like_ordinary_mention(message: &str) -> bool {
    let lower = message.to_lowercase();
    const SOFT: &[&str] = &[
        "名字不错",
        "这个名字",
        "挺好",
        "很好听",
        "nice name",
        "名字好听",
    ];
    SOFT.iter()
        .any(|phrase| message.contains(phrase) || lower.contains(phrase))
}
