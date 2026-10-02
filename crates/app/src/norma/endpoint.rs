use super::route::{WorkspaceRouteError, WorkspaceRouteIndex};
use super::server::is_managed_agent_command;
use super::transport::{NormaServerRequest, NormaSocketListener};
use crate::app::MultisoulApp;
use gpui::{App, Global, WeakEntity, Window};
use norma::client::default_socket_path;
use norma::protocol::{
    CardCommand, CardCommandResult, OrchestrationDoctorReport, WireResponse, WorkspaceSummary,
    CURRENT_PROTOCOL_VERSION,
};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

mod routing;
mod task_control;
mod task_dispatch;

pub(crate) use routing::protocol_compatibility_error;
use routing::{
    agent_error, closed_workspace_response, execute_cross_workspace, execute_in_workspace,
    execute_validated_in_workspace, receipt_state, resolve_target_definition, resolve_targets,
    response_error_code, routing_error_response, serialized_profile, source_active_task_ref,
    validate_source,
};
use task_control::spawn_global_task_control;
pub(crate) mod mobile_approval;
pub(crate) mod mobile_command;
pub(crate) mod mobile_conversation;
mod mobile_plan;
mod mobile_realtime;
pub(crate) mod mobile_review;
mod mobile_task_baseline;
pub(crate) mod mobile_terminal;
use task_dispatch::spawn_global_dispatch;

const ENDPOINT_POLL_INTERVAL: Duration = Duration::from_millis(12);

#[derive(Clone)]
struct WorkspaceTarget {
    app: WeakEntity<MultisoulApp>,
    summary: WorkspaceSummary,
}

impl WorkspaceTarget {
    fn with_optional_window<R>(
        &self,
        cx: &mut App,
        update: impl FnOnce(Option<&mut Window>, &mut App) -> R,
    ) -> anyhow::Result<R> {
        let app = self
            .app
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("project released"))?;
        let window = app
            .read(cx)
            .project_window
            .filter(|window| cx.windows().contains(window));
        match window {
            Some(window) => window.update(cx, |_, window, cx| update(Some(window), cx)),
            None => Ok(update(None, cx)),
        }
    }
}

struct RoutedTargets {
    source: WorkspaceTarget,
    target: WorkspaceTarget,
}

pub(crate) struct NormaEndpointHost {
    listener: NormaSocketListener,
    routes: WorkspaceRouteIndex<WorkspaceTarget>,
    task_store: Option<storage::OrchestrationTaskRouteStore>,
}

impl Global for NormaEndpointHost {}

impl NormaEndpointHost {
    fn bind(socket_path: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            listener: NormaSocketListener::bind(socket_path)?,
            routes: WorkspaceRouteIndex::default(),
            task_store: None,
        })
    }
}

pub(crate) fn attach_task_store(store: storage::OrchestrationTaskRouteStore, cx: &mut App) {
    if !cx.has_global::<NormaEndpointHost>() {
        return;
    }
    let host = cx.global_mut::<NormaEndpointHost>();
    if host.task_store.is_some() {
        return;
    }
    host.task_store = Some(store.clone());
    cx.background_executor()
        .spawn(async move {
            if let Err(error) = store.reconcile_non_terminal_as_interrupted() {
                tracing::warn!(%error, "failed to reconcile global orchestration tasks at startup");
            }
        })
        .detach();
}

pub(crate) fn install(cx: &mut App) -> std::io::Result<()> {
    if cx.has_global::<NormaEndpointHost>() {
        return Ok(());
    }
    cx.set_global(NormaEndpointHost::bind(default_socket_path())?);
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(ENDPOINT_POLL_INTERVAL).await;
        if cx.update(drain_requests).is_err() {
            break;
        }
    })
    .detach();
    Ok(())
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) fn install_at_path(socket_path: PathBuf, cx: &mut App) -> std::io::Result<()> {
    if cx.has_global::<NormaEndpointHost>() {
        return Ok(());
    }
    cx.set_global(NormaEndpointHost::bind(socket_path)?);
    Ok(())
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn submit_for_e2e(
    request: norma::protocol::WireRequest,
    cx: &mut App,
) -> crossbeam_channel::Receiver<WireResponse> {
    let (response, receiver) = crossbeam_channel::bounded(1);
    dispatch_request(NormaServerRequest { request, response }, cx);
    receiver
}

pub(crate) struct WorkspaceRegistration<'a> {
    pub(crate) workspace_id: &'a str,
    pub(crate) route_id: &'a str,
    pub(crate) app: WeakEntity<MultisoulApp>,
    pub(crate) display_name: String,
    pub(crate) canonical_root: PathBuf,
    pub(crate) agent_count: usize,
}

pub(crate) fn register_workspace(
    registration: WorkspaceRegistration<'_>,
    cx: &mut App,
) -> Result<(), WorkspaceRouteError> {
    crate::mobile::project_events::changed();
    let WorkspaceRegistration {
        workspace_id,
        route_id,
        app,
        display_name,
        canonical_root,
        agent_count,
    } = registration;
    if !cx.has_global::<NormaEndpointHost>() {
        return Err(WorkspaceRouteError::EndpointUnavailable);
    }
    let host = cx.global_mut::<NormaEndpointHost>();
    host.routes.register(
        workspace_id,
        route_id,
        WorkspaceTarget {
            app,
            summary: WorkspaceSummary {
                workspace_id: workspace_id.to_owned(),
                display_name,
                canonical_root,
                active: true,
                route_state: "open".to_string(),
                agent_count,
            },
        },
    )
}

pub(crate) fn unregister_workspace(workspace_id: &str, route_id: &str, cx: &mut App) -> bool {
    crate::mobile::project_events::changed();
    if !cx.has_global::<NormaEndpointHost>() {
        return false;
    }
    let host = cx.global_mut::<NormaEndpointHost>();
    let removed = host.routes.unregister(workspace_id, route_id);
    let task_store = removed.then(|| host.task_store.clone()).flatten();
    if let Some(task_store) = task_store {
        let workspace_id = workspace_id.to_owned();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = task_store.interrupt_target_workspace(&workspace_id) {
                    tracing::warn!(%error, %workspace_id, "failed to interrupt closed Workspace tasks");
                }
            })
            .detach();
    }
    removed
}

pub(crate) fn new_workspace_route_id() -> std::io::Result<String> {
    let mut bytes = [0_u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    // RFC 4122 v4: version nibble = 0100, variant high bits = 10.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    ))
}

pub(crate) fn workspace_summaries(cx: &App) -> Vec<WorkspaceSummary> {
    let Some(host) = cx.try_global::<NormaEndpointHost>() else {
        return Vec::new();
    };
    let mut workspaces = host
        .routes
        .active_targets()
        .map(|target| target.summary.clone())
        .collect::<Vec<_>>();
    workspaces.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    workspaces
}

/// Read only the authorized projects supplied by the mobile background owner. Reuse the exact
/// desktop route and check installation + current workspace again; stale project entities fail closed.
pub(crate) fn mobile_project_snapshots(
    database: &std::path::Path,
    mut projects: Vec<norma::mobile::ProjectSnapshot>,
    include_cards: bool,
    agents: Option<crossbeam_channel::Sender<Vec<agent::AgentRosterEntry>>>,
    cx: &mut App,
) -> Vec<norma::mobile::ProjectSnapshot> {
    // A roster has one workspace owner. Catalog/multi-project reads cannot mix agent identities.
    let mut agents = agents.filter(|_| projects.len() == 1);
    for project in &mut projects {
        let Some(host) = cx.try_global::<NormaEndpointHost>() else {
            continue;
        };
        let target = host.routes.resolve_active(&project.workspace_id).cloned();
        project.state = "closed".into();
        let Ok(target) = target else {
            continue;
        };
        let data = target
            .app
            .update(cx, |app, _| {
                if app.database_paths.app_db_path() != database
                    || app.workspace_id.as_deref() != Some(&project.workspace_id)
                {
                    return None;
                }
                let route = app.workspace_route_id.clone()?;
                let count = app
                    .runtime
                    .document
                    .nodes()
                    .iter()
                    .filter(|n| matches!(n.kind, canvas::CanvasNodeKind::Agent(_)))
                    .count();
                let cards = include_cards.then(|| app.mobile_card_snapshots());
                let roster = agents.as_ref().map(|_| app.agent_roster_for_provider());
                Some((route, count, cards, roster))
            })
            .ok()
            .flatten();
        if let Some((route, count, cards, roster)) = data {
            project.state = "open".into();
            project.route_id = Some(route);
            project.card_count = Some(count);
            project.cards = cards;
            if let (Some(reply), Some(roster)) = (agents.take(), roster) {
                let _ = reply.try_send(roster);
            }
        }
    }
    projects
}

pub(crate) fn doctor_report(client_version: Option<&str>, cx: &App) -> OrchestrationDoctorReport {
    let workspaces = workspace_summaries(cx);
    OrchestrationDoctorReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        cli_version: client_version.unwrap_or("unknown").to_string(),
        protocol_version: CURRENT_PROTOCOL_VERSION,
        socket: if cx.has_global::<NormaEndpointHost>() {
            "ready".to_string()
        } else {
            "unavailable".to_string()
        },
        hook_ingress: crate::settings::agent_hook_ingress::health(cx).to_string(),
        provider_health: crate::settings::orchestration::health::provider_health(cx).to_string(),
        skill_health: crate::settings::orchestration::health::skill_health(cx).to_string(),
        open_workspace_count: workspaces.len(),
    }
}

fn drain_requests(cx: &mut App) {
    loop {
        let request = cx
            .try_global::<NormaEndpointHost>()
            .and_then(|host| host.listener.try_recv());
        let Some(request) = request else {
            break;
        };
        dispatch_request(request, cx);
    }
}

fn dispatch_request(server_request: NormaServerRequest, cx: &mut App) {
    let request_id = server_request.request.request_id;
    let response = server_request.response;
    let request = server_request.request;
    if let Some(error) = protocol_compatibility_error(
        request.protocol_version,
        is_managed_agent_command(&request.command),
    ) {
        let _ = response.send(agent_error(
            request_id,
            error,
            "Norma CLI 与 App 编排协议不兼容；请修复安装并重启 Agent Session",
            false,
        ));
        return;
    }
    if matches!(request.command, CardCommand::Doctor) {
        let _ = response.send(WireResponse::new(
            request_id,
            Ok(CardCommandResult::Doctor {
                report: doctor_report(request.client_version.as_deref(), cx),
            }),
        ));
        return;
    }
    if is_managed_agent_command(&request.command)
        && crate::settings::orchestration::health::is_checking(cx)
    {
        let _ = response.send(agent_error(
            request_id,
            norma::protocol::AgentErrorCode::OrchestrationChecking,
            "App 全局编排健康检查尚未完成",
            true,
        ));
        return;
    }
    if matches!(request.command, CardCommand::DispatchAgentTasks { .. }) {
        spawn_global_dispatch(request, response, cx);
        return;
    }
    if matches!(
        request.command,
        CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskResult { .. }
            | CardCommand::AgentTaskCancel { .. }
    ) {
        spawn_global_task_control(request, response, cx);
        return;
    }

    let routed = resolve_targets(&request, cx);
    let wire_response = match routed {
        Ok(routed) if routed.source.summary.workspace_id == routed.target.summary.workspace_id => {
            execute_in_workspace(routed.target, request, cx)
        }
        Ok(routed) => execute_cross_workspace(routed, request, cx),
        Err(error) => routing_error_response(request_id, error),
    };
    let _ = response.send(wire_response);
}
