use super::operation::non_blank_task;
use crate::protocol::{StaffOp, TaskPhase};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub(super) struct StaffArgs {
    #[command(subcommand)]
    command: StaffCommand,
}

#[derive(Debug, Subcommand)]
enum StaffCommand {
    Create {
        #[arg(long, value_parser = non_blank_task)]
        title: String,
    },
    List,
    Reply {
        task_id: String,
        #[arg(long, value_parser = non_blank_task)]
        text: String,
    },
    Phase {
        task_id: String,
        #[arg(value_enum)]
        phase: Phase,
    },
    Propose {
        task_id: String,
        agent: String,
        #[arg(long, value_parser = non_blank_task)]
        title: String,
        #[arg(long, value_parser = non_blank_task)]
        prompt: String,
        #[arg(long, value_parser = non_blank_task)]
        why: String,
    },
    Assign {
        task_id: String,
        agent: String,
        #[arg(long, value_parser = non_blank_task)]
        title: String,
        #[arg(long, value_parser = non_blank_task)]
        prompt: String,
        #[arg(long, value_parser = non_blank_task)]
        why: String,
    },
    /// 只能归档；取消归档只在面板里由用户操作。
    Archive {
        task_id: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Phase {
    Active,
    Merging,
    Done,
}

impl StaffArgs {
    pub(super) fn into_op(self) -> StaffOp {
        match self.command {
            StaffCommand::Create { title } => StaffOp::Create { title },
            StaffCommand::List => StaffOp::List,
            StaffCommand::Reply { task_id, text } => StaffOp::Reply { task_id, text },
            StaffCommand::Propose {
                task_id,
                agent,
                title,
                prompt,
                why,
            } => StaffOp::Propose {
                task_id,
                agent,
                title,
                prompt,
                why,
            },
            StaffCommand::Assign {
                task_id,
                agent,
                title,
                prompt,
                why,
            } => StaffOp::Assign {
                task_id,
                agent,
                title,
                prompt,
                why,
            },
            StaffCommand::Archive { task_id } => StaffOp::Archive {
                task_id,
                archived: true,
            },
            StaffCommand::Phase { task_id, phase } => StaffOp::Phase {
                task_id,
                phase: match phase {
                    Phase::Active => TaskPhase::Active,
                    Phase::Merging => TaskPhase::Merging,
                    Phase::Done => TaskPhase::Done,
                },
            },
        }
    }
}
