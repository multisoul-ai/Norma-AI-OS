use super::*;
use norma::claude::{read_transcript, TranscriptCursor, TranscriptPage};

/// Bound by Runtime before scheduling IO. Remote callers cannot supply a filesystem path.
pub struct MobileClaudeHistoryRead {
    path: PathBuf,
    pub(super) projects_root: PathBuf,
    session_id: String,
    workspace: PathBuf,
    control: Arc<launch::LaunchControl>,
}

impl MobileClaudeHistoryRead {
    pub(super) fn is_closed(&self) -> bool {
        self.control.is_cancelled()
    }

    pub(super) fn end(&self) -> Result<TranscriptCursor, String> {
        if self.is_closed() {
            return Err("controlled Claude run closed".into());
        }
        let cursor = norma::claude::transcript_end(
            &self.path,
            &self.projects_root,
            &self.session_id,
            &self.workspace,
        )?;
        if self.is_closed() {
            return Err("controlled Claude run closed".into());
        }
        Ok(cursor)
    }

    /// Blocking file IO. Cancellation is checked both before and after the read.
    pub fn read(
        &self,
        cursor: Option<TranscriptCursor>,
        limit: usize,
    ) -> Result<TranscriptPage, String> {
        if self.control.is_cancelled() {
            return Err("controlled Claude run closed".into());
        }
        let page = read_transcript(
            &self.path,
            &self.projects_root,
            &self.session_id,
            &self.workspace,
            cursor,
            limit,
        )?;
        if self.control.is_cancelled() {
            return Err("controlled Claude run closed".into());
        }
        Ok(page)
    }
}

impl Runtime {
    pub fn mobile_claude_turn_id(&self, node_id: NodeId, run_id: &str) -> Option<&str> {
        if !matches!(self.document.node(node_id).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Claude)
        {
            return None;
        }
        self.mobile_runs
            .get(&node_id)
            .filter(|run| {
                run.run_id.as_deref() == Some(run_id)
                    && (run.claude.is_some() || run.native)
                    && !run.control.is_cancelled()
            })?
            .prompt_id
            .as_deref()
    }

    pub fn mobile_agent_run_id(&self, node_id: NodeId) -> Option<&str> {
        self.mobile_runs.get(&node_id)?.run_id.as_deref()
    }

    /// Caller authorizes the project first. This only captures memory; use read() off-thread.
    pub fn mobile_claude_history(
        &self,
        node_id: NodeId,
        run_id: &str,
    ) -> Result<MobileClaudeHistoryRead, String> {
        if !matches!(self.document.node(node_id).map(|node|&node.kind),Some(CanvasNodeKind::Agent(spec)) if spec.profile==AgentProfile::Claude && (spec.mobile_control || self.mobile_runs.get(&node_id).is_some_and(MobileRun::is_native)))
        {
            return Err("controlled Claude card unavailable".into());
        }
        let run = self
            .mobile_runs
            .get(&node_id)
            .filter(|run| run.run_id.as_deref() == Some(run_id) && !run.control.is_cancelled())
            .ok_or("controlled Claude run unavailable")?;
        Ok(MobileClaudeHistoryRead {
            path: run
                .transcript_path
                .clone()
                .ok_or("Claude has not provided a transcript path")?,
            projects_root: claude_projects_root()?,
            session_id: run.session_id.clone().ok_or("Claude session unavailable")?,
            workspace: self.workspace_root.clone().ok_or("project unavailable")?,
            control: run.control.clone(),
        })
    }
}

pub(super) fn hook_path(
    run: &MobileRun,
    event: &norma::agent_hook::AgentHookEvent,
) -> Result<Option<PathBuf>, ()> {
    let Some(value) = event
        .raw_json
        .get("transcript_path")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    // PATH_MAX-sized input guard; canonical containment and file contents are checked off-thread.
    let text = value
        .as_str()
        .filter(|value| value.len() <= 4096)
        .ok_or(())?;
    let path = PathBuf::from(text);
    let expected = format!("{}.jsonl", run.session_id.as_deref().ok_or(())?);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || path.file_name().and_then(|s| s.to_str()) != Some(&expected)
        || run
            .transcript_path
            .as_ref()
            .is_some_and(|current| current != &path)
    {
        return Err(());
    }
    Ok(Some(path))
}

impl MobileRun {
    pub(super) fn record_transcript(
        &mut self,
        node_id: NodeId,
        path: PathBuf,
    ) -> Result<(), String> {
        if self.transcript_path.is_none() {
            if let Some(claude) = &mut self.claude {
                claude.record_transcript(
                    storage::MobileSessionKey {
                        node_id,
                        profile: "Claude".into(),
                        run_id: self.run_id.clone().ok_or("missing run")?,
                        session_id: self.session_id.clone().ok_or("missing session")?,
                    },
                    path.clone(),
                )?;
            }
            self.transcript_path = Some(path);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile_claude_history_tests.rs"]
mod tests;

pub(super) fn claude_projects_root() -> Result<PathBuf, String> {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
        .ok_or("Claude config root unavailable")?;
    Ok(config.join("projects"))
}
