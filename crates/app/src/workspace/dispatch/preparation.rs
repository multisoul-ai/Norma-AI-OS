//! Prepare only the frozen card. Capture its first ready fingerprint after background launch.
use super::*;
use crate::app::MultisoulApp;
use gpui::Entity;
use std::time::{Duration, Instant};

pub(super) async fn prepare(
    session: &Entity<MultisoulApp>,
    target: &FrozenWorkspaceTarget,
    hook_paths: &norma::agent_hook::HookInstallPaths,
    cx: &mut gpui::AsyncApp,
) -> Result<(CardExecutionFingerprint, bool), String> {
    let profile = session
        .read_with(cx, |app, _| {
            exact_target(app, &target.card)?;
            match &app
                .runtime
                .document
                .node(target.card.node_id)
                .ok_or("target_missing")?
                .kind
            {
                canvas::CanvasNodeKind::Agent(spec) => Ok::<_, String>(spec.profile),
                _ => Err("target_missing".into()),
            }
        })
        .map_err(|e| e.to_string())??;
    let paths = hook_paths.clone();
    let installed = cx
        .background_executor()
        .spawn(async move {
            let source = match profile {
                canvas::AgentProfile::Claude => norma::agent_hook::HookSource::Claude,
                canvas::AgentProfile::Codex => norma::agent_hook::HookSource::Codex,
                canvas::AgentProfile::Cursor => norma::agent_hook::HookSource::Cursor,
                canvas::AgentProfile::Grok => norma::agent_hook::HookSource::Grok,
                canvas::AgentProfile::OpenCode => norma::agent_hook::HookSource::OpenCode,
            };
            norma::agent_hook::agent_hooks_installed_for_source(source, &paths)
        })
        .await;
    let (initial, launch, needs_hook) = session
        .update(cx, |app, _| {
            let live = exact_target(app, &target.card)?;
            if target
                .execution
                .as_ref()
                .is_some_and(|e| e != &live.execution)
            {
                return Err("execution_changed".into());
            }
            if !matches!(&app.runtime.document.node(target.card.node_id).ok_or("target_missing")?.kind,
                canvas::CanvasNodeKind::Agent(spec) if spec.profile == profile) {
                return Err("configuration_changed".into());
            }
            let needs_hook = installed && app.runtime.agent_hook_config().is_some();
            let launch = if live.execution.terminal_session_id.is_none() {
                app.runtime
                    .prepare_registered_agent_terminal(target.card.node_id, 80, 24)
                    .and_then(|launch| {
                        app.runtime
                            .complete_registered_launch_with_factory(target.card.node_id, launch)
                    })
                    .map_err(|e| e.to_string())?
            } else {
                None
            };
            Ok::<_, String>((live.execution, launch, needs_hook))
        })
        .map_err(|e| e.to_string())??;
    if let Some(launch) = launch {
        let run = launch.run_id().to_owned();
        let live = cx
            .background_executor()
            .spawn(async move { launch.spawn() })
            .await
            .and_then(|p| p.into_live());
        session
            .update(cx, |app, cx| {
                let current = exact_target(app, &target.card)?;
                if current.execution.runtime_instance_id != initial.runtime_instance_id
                    || current.execution.route_generation != initial.route_generation
                {
                    return Err("execution_changed".into());
                }
                app.runtime
                    .complete_native_agent_terminal(target.card.node_id, &run, live)
                    .map_err(|e| e.to_string())?;
                app.mark_session_dirty();
                cx.notify();
                Ok::<_, String>(())
            })
            .map_err(|e| e.to_string())??;
    }
    let launched = session
        .read_with(cx, |app, _| exact_target(app, &target.card))
        .map_err(|e| e.to_string())??;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (current, observed, rpc) = session
            .read_with(cx, |app, _| {
                Ok::<_, String>((
                    exact_target(app, &target.card)?,
                    app.runtime.agent_task_state(target.card.node_id).is_some(),
                    app.runtime.workspace_card_uses_rpc(target.card.node_id),
                ))
            })
            .map_err(|e| e.to_string())??;
        let execution = current.execution;
        if execution.runtime_instance_id != launched.execution.runtime_instance_id
            || execution.route_generation != launched.execution.route_generation
            || execution.run_id != launched.execution.run_id
                && (initial.terminal_session_id.is_some() || !observed)
            || execution.terminal_session_id != launched.execution.terminal_session_id
            || target.execution.as_ref().is_some_and(|e| e != &execution)
        {
            return Err("execution_changed".into());
        }
        if execution.terminal_session_id.is_none() {
            return Err("terminal_not_ready".into());
        }
        // A persisted resume binding is not evidence that this new PTY is ready. Native
        // SessionStart may establish a new run before we capture the cold target's first identity.
        if !needs_hook || execution.session_id.is_some() && observed {
            let managed = rpc || observed && execution.session_id.is_some();
            return Ok((execution, managed));
        }
        if Instant::now() >= deadline {
            return Err("provider_not_ready".into());
        }
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
}

fn exact_target(
    app: &MultisoulApp,
    card: &WorkspaceCardRef,
) -> Result<WorkspaceCardTarget, String> {
    if app.workspace_role_scope() != Some((&card.workspace_id, &card.area_id)) {
        return Err("workspace_scope_changed".into());
    }
    let current = app
        .runtime
        .workspace_card_target(
            &card.workspace_id,
            &card.area_id,
            app.workspace_dispatch_route_generation(),
            card.node_id,
        )
        .map_err(|e| e.code().to_owned())?;
    if current.card != *card {
        return Err("card_changed".into());
    }
    Ok(current)
}
