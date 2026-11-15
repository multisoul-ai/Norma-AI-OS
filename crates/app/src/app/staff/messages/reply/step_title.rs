//! 步骤标题的展示：已知的 `norma` 子命令查 i18n 表，其余只显示工具名。快照里没有路径和参数。
use norma::protocol::StepTitle;
use ui::i18n::text as t;

pub(in crate::app::staff::messages) fn label(title: &StepTitle) -> String {
    match title {
        StepTitle::Tool(name) => name.clone(),
        StepTitle::Norma(words) => match norma_key(words) {
            Some(key) => t(key).to_owned(),
            None => format!("norma {}", words.join(" ")),
        },
    }
}

fn norma_key(words: &[String]) -> Option<&'static str> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let rest = words.strip_prefix(&["brain"]).unwrap_or(&words);
    Some(match rest {
        ["agents"] => "staff.step.agents",
        ["approvals"] => "staff.step.approvals",
        ["questions"] => "staff.step.questions",
        ["history"] => "staff.step.history",
        ["report"] => "staff.step.report",
        ["propose"] => "staff.step.propose",
        ["propose-answer"] => "staff.step.propose_answer",
        ["context"] => "staff.step.context",
        ["task", "propose"] => "staff.step.task_propose",
        ["task", "assign"] => "staff.step.task_assign",
        ["task", "phase"] => "staff.step.task_phase",
        ["task", "archive"] => "staff.step.task_archive",
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../../../../tests/unit/staff_step_title_tests.rs"]
mod tests;
