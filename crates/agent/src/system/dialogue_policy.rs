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

pub(super) fn route_user_turn(text: &str) -> DialogueRoutingDecision {
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
    } else {
        DialogueRoutingDecision::ModelOnly
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
