use super::{Auth, Authenticated, CardSigner};
use rmcp::{model::*, service::RequestContext, ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
mod model;
mod pagination;
mod schema;
pub use model::{Card, Project, Snapshot, Task, Workspace};
use model::{Directory, ListTasks, Progress};
use pagination::Page;

/// Snapshot acquisition is read-only. Implementations must never ensure/load a Runtime.
pub trait DataSource: Send + Sync {
    fn snapshot(&self) -> anyhow::Result<Snapshot>;
}
#[derive(Clone)]
pub struct ConnectorTools {
    auth: Arc<Auth>,
    signer: CardSigner,
    source: Arc<dyn DataSource>,
    pages: Arc<Mutex<HashMap<String, Page>>>,
}
impl ConnectorTools {
    pub fn new(auth: Arc<Auth>, signer: CardSigner, source: Arc<dyn DataSource>) -> Self {
        Self {
            auth,
            signer,
            source,
            pages: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn definitions() -> Vec<Tool> {
        schema::definitions()
    }
    pub fn call(&self, identity: &Authenticated, name: &str, input: Value) -> CallToolResult {
        let sessions = self.auth.sessions();
        let parameters = format!(
            "sha256:{}",
            hex::encode(sha2::Sha256::digest(
                serde_json::to_vec(&input).expect("JSON parameters")
            ))
        );
        let value = match self.auth.read(identity) {
            Ok(()) => self
                .execute(name, input)
                .unwrap_or_else(|code| schema::error(code, None)),
            Err((code, retry)) => schema::error(code, retry),
        };
        let code = value
            .get("error")
            .and_then(|e| e["code"].as_str())
            .unwrap_or("ok");
        if self
            .auth
            .audit_parameters(
                if matches!(name, "directory" | "list_tasks" | "get_task_progress") {
                    name
                } else {
                    "unknown"
                },
                &parameters,
                code,
            )
            .is_err()
        {
            return result(schema::error("internal", None));
        }
        // A revoke during disk/capture work must not release the final snapshot.
        if sessions.is_cancelled() {
            return result(schema::error("unauthorized", None));
        }
        result(value)
    }
    fn execute(&self, name: &str, input: Value) -> Result<Value, &'static str> {
        match name {
            "directory" => {
                let args: Directory =
                    serde_json::from_value(input).map_err(|_| "invalid_argument")?;
                let mut snapshot = self.source.snapshot().map_err(|_| "internal")?;
                self.project(&mut snapshot);
                if let Some(id) = args.workspace_id {
                    snapshot.workspaces.retain(|w| w.workspace_id == id);
                    if snapshot.workspaces.is_empty() {
                        return Err("workspace_not_found");
                    }
                }
                for workspace in &mut snapshot.workspaces {
                    if !args.include_idle {
                        workspace.cards.retain(|c| c.state != "idle");
                    }
                }
                Ok(json!({"observed_at":self.auth.now(),"workspaces":snapshot.workspaces}))
            }
            "list_tasks" => {
                let args: ListTasks =
                    serde_json::from_value(input).map_err(|_| "invalid_argument")?;
                if args.limit == 0
                    || args.limit > 50
                    || args.since.is_some_and(|s| s < 0)
                    || !schema::valid_states(&args.states)
                {
                    return Err("invalid_argument");
                }
                if let Some(card) = &args.card_id {
                    self.signer.verify(card)?;
                }
                self.page(args)
            }
            "get_task_progress" => {
                let args: Progress =
                    serde_json::from_value(input).map_err(|_| "invalid_argument")?;
                if args.task_id.is_some() == args.operation_id.is_some()
                    || args
                        .task_id
                        .as_ref()
                        .or(args.operation_id.as_ref())
                        .is_some_and(|s| s.is_empty() || s.len() > 4096)
                {
                    return Err("invalid_argument");
                }
                let snapshot = self.source.snapshot().map_err(|_| "internal")?;
                let task = snapshot
                    .tasks
                    .into_iter()
                    .find(|t| {
                        args.task_id.as_ref().is_some_and(|id| id == &t.task_id)
                            || args
                                .operation_id
                                .as_ref()
                                .is_some_and(|id| t.operation_id.as_ref() == Some(id))
                    })
                    .ok_or("task_not_found")?;
                let progress = task
                    .progress
                    .filter(|p| p.total > 0 && p.completed <= p.total)
                    .map(|mut p| {
                        p.current_step = p.current_step.map(|step| bounded(&step));
                        p
                    });
                Ok(
                    json!({"task_id":task.task_id,"state":task.state,"tracking":task.tracking,"progress":progress,"last_activity_at":task.updated_at,"pending_approval":task.pending_approval,"result_summary":task.result_summary.map(|s|bounded(&s)),"error":task.error.map(|s|bounded(&s))}),
                )
            }
            _ => Err("invalid_argument"),
        }
    }
    fn project(&self, snapshot: &mut Snapshot) {
        for workspace in &mut snapshot.workspaces {
            let mut names = HashMap::<String, usize>::new();
            for card in &workspace.cards {
                *names.entry(card.role_name.to_lowercase()).or_default() += 1;
            }
            for card in &mut workspace.cards {
                card.card_id = self.signer.sign(&card.identity);
                card.role_name_unique = names[&card.role_name.to_lowercase()] == 1;
                if !card.loaded {
                    card.state = "unloaded".into();
                    card.current_task_id = None;
                }
            }
        }
        for task in &mut snapshot.tasks {
            task.summary = bounded(&task.summary);
        }
    }
}
impl ServerHandler for ConnectorTools {
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(&[
            ProtocolVersion::V_2025_11_25,
            ProtocolVersion::V_2025_06_18,
            ProtocolVersion::V_2025_03_26,
        ])
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: Self::definitions(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let identity = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|p| p.extensions.get::<Authenticated>())
            .cloned();
        let Some(identity) = identity else {
            return Ok(result(schema::error("unauthorized", None)).into());
        };
        let tools = self.clone();
        let session = self.auth.sessions();
        let output = tokio::task::spawn_blocking(move || {
            tools.call(
                &identity,
                &request.name,
                Value::Object(request.arguments.unwrap_or_default()),
            )
        })
        .await;
        if session.is_cancelled() {
            return Ok(result(schema::error("unauthorized", None)).into());
        }
        Ok(output
            .unwrap_or_else(|_| result(schema::error("internal", None)))
            .into())
    }
}
/// MCP backward compatibility: clients that ignore `structuredContent` (e.g. Grok Bot
/// custom connectors) read `content[0].text`, so it carries the same JSON document.
fn result(value: Value) -> CallToolResult {
    let error = value.get("error").is_some();
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".into());
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result.is_error = Some(error);
    result
}
pub fn bounded(text: &str) -> String {
    text.chars().take(500).collect()
}
