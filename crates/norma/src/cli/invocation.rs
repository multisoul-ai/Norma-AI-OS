use super::{AgentCliCommand, CliInvocation, NormaArgs, OutputFormat, TopCommand};
use crate::protocol::CardCommand;

impl NormaArgs {
    pub(super) fn into_invocation(self) -> CliInvocation {
        let explicit_format = self.format;
        match self.command {
            TopCommand::Doctor => CliInvocation {
                command: CardCommand::Doctor,
                format: explicit_format.unwrap_or(OutputFormat::Text),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
                brain_scope: None,
            },
            TopCommand::Workspace(workspace) => CliInvocation {
                command: workspace.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
                brain_scope: None,
            },
            TopCommand::Card(card) => CliInvocation {
                command: card.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Text),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
                brain_scope: None,
            },
            TopCommand::Agent(agent) => {
                let project_self = matches!(agent.command, AgentCliCommand::Myself);
                let (command, wait_timeout_seconds, target_workspace_id, operation_id) =
                    agent.into_command();
                CliInvocation {
                    command,
                    format: explicit_format.unwrap_or(OutputFormat::Json),
                    wait_timeout_seconds,
                    target_workspace_id,
                    operation_id,
                    project_self,
                    brain_scope: None,
                }
            }
            TopCommand::Present(present) => CliInvocation {
                command: present.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
                brain_scope: None,
            },
            TopCommand::Brain(brain) => {
                let (command, brain_scope) = brain.into_command();
                CliInvocation {
                    command,
                    brain_scope,
                    format: explicit_format.unwrap_or(OutputFormat::Json),
                    wait_timeout_seconds: None,
                    target_workspace_id: None,
                    operation_id: None,
                    project_self: false,
                }
            }
            TopCommand::Ego(ego) => CliInvocation {
                command: ego.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
                brain_scope: None,
            },
        }
    }
}
