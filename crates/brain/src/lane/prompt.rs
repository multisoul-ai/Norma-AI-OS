//! 把事件渲染成发给大脑的文字。纯函数。

use crate::event::{ApprovalRequest, Outcome};
use std::collections::BTreeMap;

const MAX_EXCERPT_CHARS: usize = 600;

/// 只截断长度，保留换行；用于用户原话。
fn truncate(text: &str) -> String {
    let head: String = text.chars().take(MAX_EXCERPT_CHARS * 3).collect();
    if head.chars().count() < text.chars().count() {
        format!("{head}…")
    } else {
        head
    }
}

fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_EXCERPT_CHARS {
        return flat;
    }
    let head: String = flat.chars().take(MAX_EXCERPT_CHARS).collect();
    format!("{head}…")
}

pub fn finished_line(agent: &str, outcome: Outcome, excerpt: &str) -> String {
    let state = match outcome {
        Outcome::Succeeded => "已完成",
        Outcome::Failed => "失败",
        Outcome::Interrupted => "被中断",
    };
    format!("- {agent} {state}：{}", clip(excerpt))
}

pub fn finished_batch(lines: &[String]) -> String {
    format!(
        "【事件】以下 Agent 刚有结果：\n{}\n请整理后用 report 命令告诉用户；需要时派后续任务。",
        lines.join("\n")
    )
}

pub fn summary(digest: &BTreeMap<String, String>) -> String {
    let lines: Vec<String> = digest
        .iter()
        .map(|(agent, text)| format!("- {agent}：{}", clip(text)))
        .collect();
    format!(
        "【定时摘要】这段时间各 Agent 的最新进展：\n{}\n用一两句话向用户汇报整体进度；没有值得说的就什么都不说。",
        lines.join("\n")
    )
}

pub fn user_request(text: &str) -> String {
    format!(
        "【用户】{}\n处理后用 report 命令（--priority now）回复用户。",
        truncate(text)
    )
}

pub fn goal(text: &str) -> String {
    format!(
        "【目标】用户设定的目标：{}\n记住它，作为后续判断的依据。",
        truncate(text)
    )
}

pub fn approval(request: &ApprovalRequest, norma_bin: &str) -> String {
    let subject = &request.subject;
    let what = subject
        .command
        .clone()
        .or_else(|| subject.file_path.as_ref().map(|p| p.display().to_string()))
        .unwrap_or_default();
    format!(
        "【审批】审批 {id}：{agent} 请求使用 {tool}：{what}\n项目目录：{root}\n判断是否应放行，然后执行恰好一条命令：\n`{bin} approval decide {id} allow|deny|escalate --reason \"一句话理由\"`\n拿不准就用 escalate。",
        bin = norma_bin,
        id = request.id,
        agent = request.agent,
        tool = subject.tool_name,
        what = clip(&what),
        root = subject.project_root.display(),
    )
}

/// 会话创建后发送的第一条消息，说明角色与规则。
pub fn primer(role: Role, norma_bin: &str, goal: Option<&str>) -> String {
    let duty = match role {
        Role::Main => "你是 Norma 的后台大脑，负责整理各 Agent 的进度、回答用户转来的问题、给空闲 Agent 派后续任务。",
        Role::Approval => "你是 Norma 的审批助手，只判断 Agent 的审批请求是否应放行。",
    };
    let goal = goal.map(|g| format!("\n当前目标：{g}")).unwrap_or_default();
    format!(
        "{duty}\n只能通过终端执行 `{norma_bin}` 命令行动，一次一条，不要使用管道、重定向或其他命令；不要修改任何文件。\n常用命令：`{norma_bin} agent list --format json`、`{norma_bin} agent task assign <agent> --task <文字> --operation-id <uuid>`、`{norma_bin} report --text <文字> [--priority now|normal]`。\n汇报要简短口语化，一两句话，不要念路径和命令。没有变化就不要汇报。{goal}\n回复“收到”即可。"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Main,
    Approval,
}
