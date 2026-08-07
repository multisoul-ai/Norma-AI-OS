use super::*;

fn resolve_agent_restore_session_id(
    node_session: Option<&NodeSession>,
    discovered_session_id: Option<&str>,
) -> Option<String> {
    let saved_session_id = node_session
        .filter(|session| session.kind == NodeSessionKind::Agent)
        .and_then(|session| session.session_id.as_deref())
        .filter(|session_id| !session_id.is_empty());
    saved_session_id
        .or_else(|| discovered_session_id.filter(|session_id| !session_id.is_empty()))
        .map(ToString::to_string)
}

pub fn build_agent_restore_command_for_test(
    session: &NodeSession,
    profile: AgentProfile,
    hook_env: Option<&AgentHookEnv>,
    workspace_root: Option<&Path>,
) -> PtyCommand {
    build_agent_restore_command_with_discovery_for_test(
        session,
        profile,
        hook_env,
        workspace_root,
        None,
    )
}

pub fn build_agent_restore_command_with_discovery_for_test(
    session: &NodeSession,
    profile: AgentProfile,
    hook_env: Option<&AgentHookEnv>,
    workspace_root: Option<&Path>,
    discovered_session_id: Option<&str>,
) -> PtyCommand {
    let session_id = resolve_agent_restore_session_id(Some(session), discovered_session_id);
    match session_id.as_deref() {
        Some(id) => session_snapshot::build_agent_resume_command(
            profile,
            DEFAULT_TERMINAL_SHELL,
            id,
            hook_env,
            workspace_root,
        ),
        None => agent_command(profile, hook_env),
    }
}

pub fn build_terminal_restore_command_for_test(session: &NodeSession, shell: &str) -> PtyCommand {
    match &session.cwd {
        Some(cwd) => session_snapshot::build_terminal_restore_command(shell, cwd),
        None => terminal_shell_command(shell),
    }
}
