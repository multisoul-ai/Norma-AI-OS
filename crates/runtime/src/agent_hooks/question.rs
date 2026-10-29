//! Per-process Claude settings; no global configuration changes or filesystem probes.
use super::*;

pub(super) fn attach(mut command: PtyCommand, env: &AgentHookEnv) -> PtyCommand {
    let Some(run_id) = &env.control_run_id else {
        return command;
    };
    if env.profile == AgentProfile::Claude {
        if let Some(script) = command
            .args
            .last_mut()
            .filter(|script| script.starts_with("exec claude ") && !script.contains(" --settings "))
        {
            // Installed bundles keep the helper next to the App; Cargo test executables live in deps/.
            let binary = std::env::current_exe()
                .ok()
                .and_then(|path| {
                    let mut parent = path.parent()?;
                    if parent.file_name().is_some_and(|name| name == "deps") {
                        parent = parent.parent()?;
                    }
                    Some(parent.join(norma::agent_hook::AGENT_HOOK_BINARY_NAME))
                })
                .unwrap_or_else(|| norma::agent_hook::AGENT_HOOK_BINARY_NAME.into());
            let forward = norma::agent_hook::build_hook_command(
                &binary,
                norma::agent_hook::HookSource::Claude,
            );
            let settings = serde_json::json!({"hooks": {
                "SessionStart": [{"hooks":[{"type":"command", "command":forward, "timeout":5, "onFailure":"continue"}]}],
                "SessionEnd": [{"hooks":[{"type":"command", "command":forward, "timeout":5, "onFailure":"continue"}]}],
                "PreToolUse": [{"matcher":"AskUserQuestion", "hooks":[{
                    "type":"command", "command":format!("{forward} --request-question"), "timeout":125, "onFailure":"continue"
                }]}],
                "PostToolUse": [{"matcher":"AskUserQuestion", "hooks":[{
                    "type":"command", "command":forward, "timeout":5, "onFailure":"continue"
                }]}],
                "PostToolUseFailure": [{"matcher":"AskUserQuestion", "hooks":[{
                    "type":"command", "command":forward, "timeout":5, "onFailure":"continue"
                }]}]
            }});
            script.push_str(" --settings ");
            script.push_str(&crate::session_snapshot::shell_quote(&settings.to_string()));
        }
    }
    command.env(norma::agent_hook::ENV_CONTROL_RUN_ID, run_id)
}
