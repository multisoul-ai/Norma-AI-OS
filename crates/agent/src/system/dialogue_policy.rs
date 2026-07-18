mod addressing;

use addressing::{
    delegation_task_text, divided_delegation_assignments, explicit_agent_targets,
    has_ambiguous_multi_agent_pronoun,
};

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
pub(super) fn delegation_assignments(
    intent: &str,
    targets: &[String],
    roster: &[crate::AgentRosterEntry],
) -> Option<Vec<norma::protocol::AgentTaskAssignment>> {
    if targets.len() > 1 {
        if let Some(assignments) = divided_delegation_assignments(intent, targets, roster) {
            return Some(assignments);
        }
    }
    let task = delegation_task_text(intent, roster);
    (!task.is_empty()).then(|| {
        targets
            .iter()
            .map(|name| norma::protocol::AgentTaskAssignment {
                name: name.clone(),
                task: task.clone(),
            })
            .collect()
    })
}

pub(super) fn validate_delegation_assignments(
    assignments: &[norma::protocol::AgentTaskAssignment],
    roster: &[crate::AgentRosterEntry],
    expected_targets: Option<&[String]>,
) -> Result<Vec<norma::protocol::AgentTaskAssignment>, String> {
    let mut seen = Vec::<String>::new();
    let mut normalized = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        let entry = roster
            .iter()
            .find(|entry| {
                names_match(&entry.name, &assignment.name)
                    || entry
                        .speech_alias
                        .as_deref()
                        .is_some_and(|alias| names_match(alias, &assignment.name))
            })
            .ok_or_else(|| format!("Agent {} is unknown", assignment.name))?;
        if !entry.enabled {
            return Err(format!("Agent {} is disabled", entry.name));
        }
        if assignment.task.trim().is_empty() {
            return Err(format!("Task for Agent {} is empty", entry.name));
        }
        if seen.iter().any(|name| names_match(name, &entry.name)) {
            return Err(format!("Agent {} is duplicated", entry.name));
        }
        seen.push(entry.name.clone());
        normalized.push(norma::protocol::AgentTaskAssignment {
            name: entry.name.clone(),
            task: assignment.task.trim().to_string(),
        });
    }
    if let Some(expected) = expected_targets {
        let has_exact_targets = expected.len() == seen.len()
            && expected
                .iter()
                .all(|target| seen.iter().any(|name| names_match(name, target)));
        if !has_exact_targets {
            return Err(format!(
                "DelegationTargetMismatch: expected exactly [{}], received [{}]",
                expected.join(", "),
                seen.join(", ")
            ));
        }
    }
    Ok(normalized)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DialogueRoutingDecision {
    ModelOnly,
    DialogueOnly,
    RequireCardTaskFallback { delegation_targets: Vec<String> },
}

impl DialogueRoutingDecision {
    pub(super) fn requires_card_task_fallback(&self) -> bool {
        matches!(self, Self::RequireCardTaskFallback { .. })
    }

    pub(super) fn rejects_card_task(&self) -> bool {
        matches!(self, Self::DialogueOnly)
    }

    pub(super) fn streams_dialogue_text(&self) -> bool {
        !self.requires_card_task_fallback()
    }

    pub(super) fn delegation_targets(&self) -> Option<&[String]> {
        match self {
            Self::ModelOnly | Self::DialogueOnly => None,
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
    if normalized.is_empty() {
        return DialogueRoutingDecision::ModelOnly;
    }
    if contains_any(&normalized, CREATE_NEGATIONS) {
        return DialogueRoutingDecision::DialogueOnly;
    }
    if has_ambiguous_multi_agent_pronoun(&normalized) {
        return DialogueRoutingDecision::DialogueOnly;
    }
    let delegation_targets = explicit_agent_targets(text, roster);
    if !delegation_targets.is_empty() {
        return DialogueRoutingDecision::RequireCardTaskFallback { delegation_targets };
    }
    if contains_any(&normalized, EXPLANATION_MARKERS)
        || contains_any(&normalized, EXPLANATION_NOUN_PHRASES)
    {
        return DialogueRoutingDecision::DialogueOnly;
    }
    if contains_any(&normalized, CREATE_ACTIONS) && contains_any(&normalized, CONCRETE_CARD_TARGETS)
    {
        return DialogueRoutingDecision::RequireCardTaskFallback {
            delegation_targets: Vec::new(),
        };
    }
    DialogueRoutingDecision::ModelOnly
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

fn names_match(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}
