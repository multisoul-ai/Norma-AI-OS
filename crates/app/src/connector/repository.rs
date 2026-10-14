//! Disk projection runs on the connector worker; live capture crosses a bounded GPUI channel.
use norma::connector::{
    Card, CardIdentity, CardSigner, CredentialStore, DataSource, Project, Snapshot, Task,
    TokenHash, Workspace,
};
use std::{path::PathBuf, time::Duration};
use storage::{
    connector_read::ConnectorReadStore, connector_store::ConnectorStore, DesktopProjectArea,
};

pub(super) struct Credentials(pub ConnectorStore);
impl CredentialStore for Credentials {
    fn tokens(&self) -> anyhow::Result<Vec<TokenHash>> {
        Ok(self
            .0
            .tokens()?
            .into_iter()
            .map(|t| TokenHash {
                hash: t.hash,
                expires_at: t.expires_at,
            })
            .collect())
    }
    fn replace_tokens(&self, tokens: &[TokenHash]) -> anyhow::Result<()> {
        self.0.replace_tokens(
            &tokens
                .iter()
                .map(|t| storage::connector_store::TokenHash {
                    hash: t.hash,
                    expires_at: t.expires_at,
                })
                .collect::<Vec<_>>(),
        )?;
        Ok(())
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
    pub reply: crossbeam_channel::Sender<anyhow::Result<Snapshot>>,
}
pub(super) struct Repository {
    pub database: PathBuf,
    pub signer: CardSigner,
    pub live: async_channel::Sender<LiveRequest>,
}
impl DataSource for Repository {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        let DiskSnapshot {
            snapshot,
            areas,
            births,
        } = persisted(&self.database, &self.signer)?;
        let (reply, receive) = crossbeam_channel::bounded(1);
        self.live
            .try_send(LiveRequest {
                areas,
                births,
                snapshot,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("live capture unavailable"))?;
        receive
            .recv_timeout(Duration::from_secs(8))
            .map_err(|_| anyhow::anyhow!("live capture timeout"))?
    }
}
struct DiskSnapshot {
    snapshot: Snapshot,
    areas: Vec<DesktopProjectArea>,
    births: std::collections::HashMap<(String, String, i64), i64>,
}
fn persisted(database: &std::path::Path, signer: &CardSigner) -> anyhow::Result<DiskSnapshot> {
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
