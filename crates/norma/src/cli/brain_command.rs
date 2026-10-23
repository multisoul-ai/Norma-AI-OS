use super::operation::non_blank_task;
use crate::protocol::{BrainDecision, BrainOp, CardCommand, ReportPriority};
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
    /// 请求播报一段话。
    Report {
        #[arg(long)]
        text: String,
        #[arg(long, value_enum, default_value_t = PriorityValue::Normal)]
        priority: PriorityValue,
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
            BrainCliCommand::Report { text, priority } => BrainOp::Report {
                text,
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
        };
        let token = self
            .token
            .or_else(|| std::env::var("NORMA_BRAIN_TOKEN").ok())
            .unwrap_or_default();
        CardCommand::Brain { token, op }
    }
}
