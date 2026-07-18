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
const AGENT_PROFILE_MARKERS: &[&str] = &["codex", "claude", "cursor", "opencode"];
const AMBIGUOUS_AGENT_PRONOUNS: &[&str] = &["让它", "让他", "让她", "交给它", "交给他", "交给她"];
const DELEGATION_CLAUSE_MODIFIERS: &[&str] = &[
    "我想要",
    "我希望",
    "我需要",
    "我想",
    "我要",
    "然后",
    "接着",
    "随后",
    "再",
    "并且",
    "同时",
    "帮我",
    "麻烦",
];
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

pub(super) fn divided_delegation_assignments(
    intent: &str,
    targets: &[String],
    roster: &[crate::AgentRosterEntry],
) -> Option<Vec<norma::protocol::AgentTaskAssignment>> {
    let keys = sorted_address_keys(roster);
    let mut divided = Vec::<(String, String)>::new();
    for clause in intent.split([',', '，', ';', '；']) {
        let clause = ["让", "请", "交给"]
            .iter()
            .find_map(|prefix| clause.trim().strip_prefix(prefix).map(str::trim_start))
            .unwrap_or_else(|| clause.trim());
        let Some((name, remainder)) = strip_any_name_prefix(clause, &keys) else {
            continue;
        };
        if !targets.iter().any(|target| target == name) {
            continue;
        }
        let task =
            strip_direct_address_modifiers(remainder.trim_start_matches(|character: char| {
                character.is_whitespace() || matches!(character, ':' | '：')
            }))
            .trim()
            .to_string();
        if task.is_empty() || divided.iter().any(|(existing, _)| existing == name) {
            return None;
        }
        divided.push((name.to_string(), task));
    }
    if divided.len() != targets.len() {
        return None;
    }
    targets
        .iter()
        .map(|target| {
            divided
                .iter()
                .find(|(name, _)| name == target)
                .map(|(_, task)| norma::protocol::AgentTaskAssignment {
                    name: target.clone(),
                    task: task.clone(),
                })
        })
        .collect()
}

pub(super) fn has_ambiguous_multi_agent_pronoun(text: &str) -> bool {
    let candidate_count = AGENT_PROFILE_MARKERS
        .iter()
        .map(|marker| text.match_indices(marker).count())
        .sum::<usize>();
    candidate_count > 1 && contains_any(text, AMBIGUOUS_AGENT_PRONOUNS)
}

pub(super) fn explicit_agent_targets(
    text: &str,
    roster: &[crate::AgentRosterEntry],
) -> Vec<String> {
    let trimmed = text.trim();
    if let Some(targets) = agent_group_targets(trimmed, roster) {
        return targets;
    }
    let keys = sorted_address_keys(roster);
    let mut targets = direct_target(strip_delegation_clause_modifiers(trimmed), &keys)
        .into_iter()
        .collect::<Vec<_>>();
    for clause in trimmed.split([',', '，', ';', '；']).skip(1) {
        let clause = strip_delegation_clause_modifiers(clause.trim_start());
        if let Some(target) = direct_target(clause, &keys) {
            push_unique(&mut targets, target);
        }
    }
    targets
}

pub(super) fn delegation_task_text(intent: &str, roster: &[crate::AgentRosterEntry]) -> String {
    let original = strip_delegation_clause_modifiers(intent.trim());
    let mut remainder = ["让", "请", "交给"]
        .iter()
        .find_map(|prefix| original.strip_prefix(prefix).map(str::trim_start))
        .unwrap_or(original);
    let keys = sorted_address_keys(roster);
    let Some((_, after_first_name)) = strip_any_name_prefix(remainder, &keys) else {
        return strip_direct_address_modifiers(original)
            .trim_matches(|character: char| {
                character.is_whitespace() || matches!(character, ',' | '，' | ':' | '：')
            })
            .to_string();
    };
    remainder = after_first_name;
    loop {
        let trimmed = remainder.trim_start();
        let Some(after_separator) = strip_any_prefix(trimmed, AGENT_GROUP_SEPARATORS) else {
            break;
        };
        let Some((_, after_name)) = strip_any_name_prefix(after_separator.trim_start(), &keys)
        else {
            break;
        };
        remainder = after_name;
    }
    let remainder = remainder.trim_start_matches(|character: char| {
        character.is_whitespace() || matches!(character, ',' | '，' | ':' | '：')
    });
    strip_direct_address_modifiers(remainder).trim().to_string()
}

fn strip_delegation_clause_modifiers(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let Some(remainder) = strip_any_prefix(trimmed, DELEGATION_CLAUSE_MODIFIERS) else {
            return trimmed;
        };
        text = remainder;
    }
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
    let (first_target, mut remainder) = strip_any_name_prefix(text, &keys)?;
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

fn contains_any(text: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| text.contains(candidate))
}
