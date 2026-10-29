use super::*;

// ── Internal attach helpers ────────────────────────────────────────────────

fn allocate_session_id(next: &mut u64) -> u64 {
    let sid = *next;
    *next += 1;
    sid
}

pub(super) fn live_grid_size(
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    node_id: NodeId,
) -> (u16, u16) {
    let Some(NodeView::Term { snapshot, .. }) = live.get(&node_id).and_then(|n| n.view()) else {
        return (DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS);
    };
    (usize_to_u16(snapshot.cols), usize_to_u16(snapshot.lines))
}

pub(super) fn attach_terminal_live(
    node_id: NodeId,
    shell: &str,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let command = terminal_shell_command(shell).with_cwd(ctx.workspace_root);
    attach_terminal_live_with_command(node_id, shell, command, cols, rows, ctx)
}

pub(super) fn attach_terminal_live_with_command(
    node_id: NodeId,
    shell: &str,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let sid = allocate_session_id(ctx.next_session_id);
    match crate::nodes::term::spawn_term_live(ctx.pty_factory, sid, command, cols, rows) {
        Ok(live) => {
            ctx.terminal_shells.insert(node_id, shell.to_string());
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Term);
            true
        }
        Err(error) => {
            tracing::warn!(
                ?node_id,
                %shell,
                %error,
                "failed to restore terminal live session"
            );
            false
        }
    }
}

pub(super) fn attach_agent_live(
    node_id: NodeId,
    profile: AgentProfile,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let hook_env = ctx.make_hook_env(node_id, profile);
    let sid = allocate_session_id(ctx.next_session_id);
    match spawn_agent_live(
        sid,
        profile,
        ctx.workspace_root,
        hook_env.as_ref(),
        cols,
        rows,
        ctx.pty_factory,
    ) {
        Ok(live) => {
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Agent);
            true
        }
        Err(error) => {
            tracing::warn!(
                ?node_id,
                ?profile,
                %error,
                "failed to restore agent live session"
            );
            false
        }
    }
}

pub(super) fn attach_agent_live_with_command(
    node_id: NodeId,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let sid = allocate_session_id(ctx.next_session_id);
    match crate::nodes::agent::spawn_agent_command(sid, command, cols, rows, ctx.pty_factory, None)
    {
        Ok(live) => {
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Agent);
            true
        }
        Err(error) => {
            tracing::warn!(
                ?node_id,
                %error,
                "failed to restore agent live session with command"
            );
            false
        }
    }
}
