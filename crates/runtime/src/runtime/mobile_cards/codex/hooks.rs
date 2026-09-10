use super::*;

/// A Provider-owned synchronous permission Hook, not a client approval request.
/// Do not expose Hook commands/output: these may contain private configuration or credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexExternalApproval {
    pub id: String,
    pub turn_id: String,
    pub source: String,
    pub source_path: String,
}

#[derive(Default)]
pub(super) struct ExternalApprovals(Vec<CodexExternalApproval>);

impl ExternalApprovals {
    pub(super) fn is_waiting(&self) -> bool {
        !self.0.is_empty()
    }

    pub(super) fn observe(
        &mut self,
        event: &Value,
        thread: &str,
        turn: Option<&str>,
    ) -> Result<(), String> {
        let params = &event["params"];
        if params["threadId"] != thread {
            return Ok(());
        }
        match event["method"].as_str() {
            Some("turn/started") if params["turn"]["id"].as_str() != turn => self.0.clear(),
            Some("turn/completed") if params["turn"]["id"].as_str() == turn => self.0.clear(),
            Some(method @ ("hook/started" | "hook/completed"))
                if turn.is_some() && params["turnId"].as_str() == turn =>
            {
                let run = &params["run"];
                if run["eventName"] != "permissionRequest" || run["executionMode"] != "sync" {
                    return Ok(());
                }
                let id = run["id"]
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 256)
                    .ok_or("invalid external Hook identity")?;
                if method == "hook/completed" {
                    self.0.retain(|hook| hook.id != id);
                    return Ok(());
                }
                let source_path = run["sourcePath"]
                    .as_str()
                    .filter(|path| !path.is_empty() && path.len() <= 4096)
                    .ok_or("invalid external Hook source")?;
                let source = run["source"].as_str().unwrap_or("unknown");
                if source.len() > 64 || run["status"] != "running" {
                    return Err("invalid external Hook start".into());
                }
                let hook = CodexExternalApproval {
                    id: id.into(),
                    turn_id: turn.expect("bound turn").into(),
                    source: source.into(),
                    source_path: source_path.into(),
                };
                if let Some(existing) = self.0.iter().find(|h| h.id == id) {
                    if existing != &hook {
                        return Err("external Hook identity changed".into());
                    }
                } else {
                    // Match Claude ingress's 64 concurrent approvals; never grow on arbitrary Hook IDs.
                    if self.0.len() >= 64 {
                        return Err("too many external permission Hooks".into());
                    }
                    self.0.push(hook);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Runtime {
    /// Caller authorizes the project. These entries are informational; no response can be sent
    /// for their IDs. Only actual Provider requestApproval messages enter the decision API.
    pub fn mobile_codex_external_approvals(
        &self,
        node_id: NodeId,
        run_id: &str,
    ) -> Result<Vec<CodexExternalApproval>, String> {
        self.mobile_runs
            .get(&node_id)
            .filter(|run| run.run_id.as_deref() == Some(run_id) && !run.control.is_cancelled())
            .and_then(|run| run.codex.as_ref())
            .map(|host| host.external_approvals.0.clone())
            .ok_or_else(|| "controlled Codex run unavailable".into())
    }
}
