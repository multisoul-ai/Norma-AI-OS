//! Disk projection runs on the connector worker; live capture crosses a bounded GPUI channel.
use norma::connector::{
    Card, CardIdentity, CardSigner, CredentialStore, DataSource, Project, Snapshot, Task,
    TokenRecord, Workspace,
};
use std::{path::PathBuf, time::Duration};
use storage::{
    connector_read::ConnectorReadStore, connector_store::ConnectorStore, DesktopProjectArea,
};

pub(super) struct Credentials(pub ConnectorStore);
impl CredentialStore for Credentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenRecord>> {
        Ok(self.0.tokens()?)
    }
    fn replace_tokens(&self, tokens: &[TokenRecord]) -> anyhow::Result<()> {
        self.0.replace_tokens(tokens)?;
        Ok(())
    }
    fn secret_or_insert(&self, name: &str, candidate: &[u8], now: i64) -> anyhow::Result<Vec<u8>> {
        Ok(self.0.secret_or_insert(name, candidate, now)?)
    }
    fn audit_auth_failure(
        &self,
        now: i64,
        source: std::net::IpAddr,
        window_start: i64,
        count: u64,
        locked_until: i64,
    ) -> anyhow::Result<()> {
        self.0
            .audit_auth_failure(now, &source.to_string(), window_start, count, locked_until)?;
        Ok(())
    }
    fn audit(&self, now: i64, tool: &str, parameters: &str, result: &str) -> anyhow::Result<()> {
        self.0.audit_parameters(now, tool, parameters, result)?;
        Ok(())
    }
}
pub(super) struct LiveRequest {
    pub areas: Vec<DesktopProjectArea>,
    pub births: std::collections::HashMap<(String, String, i64), i64>,
    pub snapshot: Snapshot,
    pub execute: Option<super::execution::ExecutionCall>,
    pub reply: crossbeam_channel::Sender<anyhow::Result<Snapshot>>,
}
pub(super) struct Repository {
    pub database: PathBuf,
    pub signer: CardSigner,
    pub live: async_channel::Sender<LiveRequest>,
}
impl DataSource for Repository {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        self.snapshot_unscoped()
    }
    fn snapshot_for(&self, epoch: &str) -> anyhow::Result<Snapshot> {
        let mut snapshot = self.snapshot_unscoped()?;
        let receipts =
            storage::MobileCommandStore::new(self.database.clone()).connector_origins()?;
        super::projection::apply(&mut snapshot, &receipts, &format!("connector:{epoch}"));
        Ok(snapshot)
    }
    fn operation_receipt(
        &self,
        epoch: &str,
        operation: &str,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let receipts = storage::MobileCommandStore::new(self.database.clone())
            .connector_receipts(&format!("connector:{epoch}"))?;
        Ok(receipts.into_iter().find(|r|r.operation_id == operation).map(|r|serde_json::json!({"operation_id":r.operation_id,"task_id":r.result.as_ref().and_then(|v|v.get("task_id")),"state":r.state,"tracking":null,"created_at":null,"updated_at":null,"state_changed_at":null,"last_activity_at":null,"events":[{"type":"accepted","occurred_at":r.created_at_unix,"state":"accepted"}],"progress":null,"pending_approval":null,"result_summary":null,"error":r.error})))
    }
    fn write(
        &self,
        epoch: &str,
        request: norma::connector::WriteRequest,
        now: i64,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<serde_json::Value, &'static str> {
        super::writes::run(self, epoch, request, now, cancellation)
    }
}
impl Repository {
    pub(super) fn snapshot_unscoped(&self) -> anyhow::Result<Snapshot> {
        self.read_live(persisted(&self.database, &self.signer)?)
    }
    pub(super) fn read_live(&self, disk: DiskSnapshot) -> anyhow::Result<Snapshot> {
        let (reply, receive) = crossbeam_channel::bounded(1);
        self.live
            .try_send(LiveRequest {
                areas: disk.areas,
                births: disk.births,
                snapshot: disk.snapshot,
                execute: None,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("live capture unavailable"))?;
        receive
            .recv_timeout(Duration::from_secs(8))
            .map_err(|_| anyhow::anyhow!("live capture timeout"))?
    }
}
pub(super) struct DiskSnapshot {
    pub snapshot: Snapshot,
    pub areas: Vec<DesktopProjectArea>,
    pub births: std::collections::HashMap<(String, String, i64), i64>,
}
pub(super) fn persisted(
    database: &std::path::Path,
    signer: &CardSigner,
) -> anyhow::Result<DiskSnapshot> {
    let mut snapshot = Snapshot::default();
    let mut areas = Vec::new();
    let mut births = std::collections::HashMap::new();
    for space in ConnectorReadStore::new(database.to_owned()).read()? {
        let mut workspace = Workspace {
            workspace_id: space.id.clone(),
            name: space.name,
            active: false,
            projects: Vec::new(),
            cards: Vec::new(),
        };
        for area in space.areas {
            for (definition, created) in &area.definition_births {
                births.insert(
                    (space.id.clone(), area.area.id.clone(), *definition),
                    *created,
                );
            }
            let project_id = hex::encode(sha2::Sha256::digest(
                area.area.project_key.to_string_lossy().as_bytes(),
            ));
            workspace.projects.push(Project {
                project_id: project_id.clone(),
                area_id: area.area.id.clone(),
                name: area.area.display_name.clone(),
                loaded: false,
            });
            for member in area.members {
                let identity = CardIdentity {
                    workspace_id: space.id.clone(),
                    area_id: area.area.id.clone(),
                    canvas_id: area.canvas_id.clone(),
                    node_id: member.node_id,
                    definition_id: member.definition_id,
                    created_at: member.created_at,
                };
                workspace.cards.push(Card {
                    card_id: signer.sign(&identity),
                    identity,
                    card_revision: "unloaded".into(),
                    role_name: member.name,
                    role_name_unique: false,
                    project_id: project_id.clone(),
                    profile: profile(&member.profile),
                    state: "unloaded".into(),
                    current_task_id: None,
                    loaded: false,
                    created_at: member.definition_created_at,
                    created_by: None,
                });
            }
            for stored in area.tasks {
                let card = workspace.cards.iter().find(|c| {
                    c.identity.area_id == area.area.id
                        && c.identity.node_id == stored.node_id
                        && c.identity.definition_id == stored.definition_id
                });
                // Deleted cards still have historical tasks. Encode their recorded target rather than redirecting.
                let identity = CardIdentity {
                    workspace_id: space.id.clone(),
                    area_id: area.area.id.clone(),
                    canvas_id: area.canvas_id.clone(),
                    node_id: stored.node_id,
                    definition_id: stored.definition_id,
                    created_at: 0,
                };
                let task_id = if stored.task_ref.is_empty() {
                    format!("t1.{}.{}", area.canvas_id, stored.id)
                } else {
                    stored.task_ref
                };
                snapshot.tasks.push(Task {
                    task_id,
                    card_id: card
                        .map(|c| c.card_id.clone())
                        .unwrap_or_else(|| signer.sign(&identity)),
                    role_name: card
                        .map(|c| c.role_name.clone())
                        .unwrap_or_else(|| "已移除卡片".into()),
                    workspace_id: space.id.clone(),
                    state: stored.state,
                    tracking: if stored.tracking == "managed" {
                        "tracked"
                    } else {
                        "untracked"
                    }
                    .into(),
                    summary: stored.summary,
                    created_at: stored.created_at,
                    updated_at: stored.updated_at,
                    source: "other".into(),
                    state_changed_at: stored.events.first().map(|e| e.0),
                    events: stored
                        .events
                        .into_iter()
                        .map(|(occurred_at, state)| norma::connector::TaskEvent {
                            r#type: "state_changed".into(),
                            occurred_at,
                            state: Some(state),
                        })
                        .collect(),
                    operation_id: None,
                    progress: None,
                    pending_approval: None,
                    result_summary: stored.result_summary,
                    error: stored.error,
                });
            }
            areas.push(area.area);
        }
        snapshot.workspaces.push(workspace);
    }
    Ok(DiskSnapshot {
        snapshot,
        areas,
        births,
    })
}
use sha2::Digest;
fn profile(value: &str) -> String {
    match value {
        "codex" => "Codex",
        "claude" => "Claude",
        "cursor" => "Cursor",
        "opencode" => "OpenCode",
        "grok" => "Grok",
        other => other,
    }
    .into()
}
