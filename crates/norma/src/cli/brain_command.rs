use super::operation::non_blank_task;
use crate::protocol::{BrainDecision, BrainOp, CardCommand, ContextRef, ReportPriority};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub(super) struct BrainArgs {
    /// App 为大脑进程下发的一次性 token；缺省读环境变量 NORMA_BRAIN_TOKEN。
    #[arg(long)]
    token: Option<String>,
    #[command(subcommand)]
    command: BrainCliCommand,
}

#[derive(Debug, Subcommand)]
enum BrainCliCommand {
    /// 按宿主提供的 JSON 引用补读共享上下文，不执行动作。
    Context {
        #[arg(long = "ref", value_parser = parse_context_ref)]
        reference: ContextRef,
    },
    /// 请求播报一段话。
    Report {
        #[arg(long)]
        text: String,
        #[arg(long, value_enum, default_value_t = PriorityValue::Normal)]
        priority: PriorityValue,
        /// 宿主返回的来源 JSON 引用，可重复指定；不填表示来源关联不可用。
        #[arg(long = "context-ref", value_parser = parse_context_ref)]
        sources: Vec<ContextRef>,
    },
    /// 列出等待处理的审批。
    Approvals,
    /// 对一条审批给出结论。
    Decide {
        approval_id: String,
        #[arg(value_enum)]
        decision: DecisionValue,
        #[arg(long)]
        reason: String,
    },
    /// 列出当前项目的 Agent。
    Agents,
    /// 给一个空闲的 Agent 派任务。
    Assign {
        agent: String,
        #[arg(long, value_parser = non_blank_task)]
        task: String,
    },
    /// 提议下一步，用户在概览里确认后才派发。
    Propose {
        agent: String,
        /// 一行标题，概览里收起时显示。
        #[arg(long, value_parser = non_blank_task)]
        title: String,
        /// 派发时原样发给 Agent 的完整指令，写清怎样算完成。
        #[arg(long, value_parser = non_blank_task)]
        task: String,
        /// 为什么建议做这件事。
        #[arg(long, value_parser = non_blank_task)]
        why: String,
    },
    /// 读某个 Agent 最近的完整对话（摘要不够判断时用）。
    History {
        agent: String,
        /// 最近几轮，默认 6，最多 20。
        #[arg(long)]
        last: Option<u32>,
        /// 只看含这个词的轮次，不区分大小写。
        #[arg(long)]
        grep: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PriorityValue {
    Normal,
    Now,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DecisionValue {
    Allow,
    Deny,
    Escalate,
}

impl BrainArgs {
    pub(super) fn into_command(self) -> CardCommand {
        let op = match self.command {
            BrainCliCommand::Context { reference } => BrainOp::Context { reference },
            BrainCliCommand::Report {
                text,
                priority,
                sources,
            } => BrainOp::Report {
                text,
                sources,
                priority: match priority {
                    PriorityValue::Normal => ReportPriority::Normal,
                    PriorityValue::Now => ReportPriority::Now,
                },
            },
            BrainCliCommand::Approvals => BrainOp::Approvals,
            BrainCliCommand::Decide {
                approval_id,
                decision,
                reason,
            } => BrainOp::Decide {
                approval_id,
                decision: match decision {
                    DecisionValue::Allow => BrainDecision::Allow,
                    DecisionValue::Deny => BrainDecision::Deny,
                    DecisionValue::Escalate => BrainDecision::Escalate,
                },
                reason,
            },
            BrainCliCommand::Agents => BrainOp::Agents,
            BrainCliCommand::Assign { agent, task } => BrainOp::Assign { agent, task },
            BrainCliCommand::Propose {
                agent,
                title,
                task,
                why,
            } => BrainOp::Propose {
                agent,
                title,
                task,
                why,
            },
            BrainCliCommand::History { agent, last, grep } => {
                BrainOp::History { agent, last, grep }
            }
        };
        let token = self
            .token
            .or_else(|| std::env::var("NORMA_BRAIN_TOKEN").ok())
            .unwrap_or_default();
        CardCommand::Brain { token, op }
    }
}

fn parse_context_ref(value: &str) -> Result<ContextRef, String> {
    let reference: ContextRef =
        serde_json::from_str(value).map_err(|_| "无效的上下文引用 JSON".to_owned())?;
    if reference.event_id.trim().is_empty()
        || reference.revision == 0
        || !reference.scope.is_valid()
    {
        return Err("上下文引用缺少有效身份或版本".into());
    }
    Ok(reference)
}
