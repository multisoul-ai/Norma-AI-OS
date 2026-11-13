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
    Message {
        task_id: String,
        #[arg(long, value_parser = non_blank_task)]
        text: String,
    },
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
            StaffCommand::Message { task_id, text } => StaffOp::Message { task_id, text },
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
