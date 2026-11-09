use super::{AdapterCommand, ToolContext};
use crate::AgentInputSource;

// Phrases whose 不/别 is not a constraint: A-not-A questions, courtesy and common compounds.
const NON_CONSTRAINTS: [&str; 29] = [
    "能不能",
    "要不要",
    "是不是",
    "可不可以",
    "行不行",
    "好不好",
    "会不会",
    "有没有",
    "方不方便",
    "需不需要",
    "忙不忙",
    "对不对",
    "不对",
    "不好意思",
    "对不起",
    "不错",
    "不过",
    "差不多",
    "别人",
    "别的",
    "分别",
    "特别",
    "区别",
    "识别",
    "类别",
    "级别",
    "差别",
    "个别",
    "告别",
];
const CONSTRAINTS: [&str; 15] = [
    "不",
    "别",
    "勿",
    "禁",
    "莫",
    "无需",
    "没必要",
    "千万",
    "只读",
    "保持",
    "not",
    "n't",
    "never",
    "avoid",
    "without",
];

/// The task text is the model's verbatim copy of the user's words. If that copy lost a
/// negation ("别改数据库"), the receiving agent must still see the original constraint.
pub(super) fn preserve_doubao_constraints(command: &mut AdapterCommand, context: &ToolContext) {
    let original = context.user_input.trim();
    if context.input_source != AgentInputSource::DoubaoVoice || original.is_empty() {
        return;
    }
    let text = match command {
        AdapterCommand::AssignAgentTask { task, .. }
        | AdapterCommand::AssignWorkspaceTask { task, .. }
        | AdapterCommand::CreateAgent {
            task: Some(task), ..
        } => task,
        AdapterCommand::ReplyAgent { message, .. } => message,
        AdapterCommand::HandoffToBrain { request } => request,
        _ => return,
    };
    if !text.contains(original) && constraints(text) < constraints(original) {
        text.push_str("\n\n用户原话（核对限制，只做派给你的部分）：");
        text.push_str(original);
    }
}

fn constraints(text: &str) -> usize {
    let mut text = text.to_lowercase();
    for phrase in NON_CONSTRAINTS {
        text = text.replace(phrase, "");
    }
    CONSTRAINTS
        .iter()
        .map(|word| text.matches(word).count())
        .sum()
}
