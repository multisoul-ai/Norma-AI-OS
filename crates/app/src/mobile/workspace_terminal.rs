use norma::mobile::{WorkspaceTerminalFrame, WorkspaceTerminalRequest, WorkspaceTerminalUpdate};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct CaptureCache {
    entries: VecDeque<Entry>,
}
struct Entry {
    id: String,
    device: String,
    scope: String,
    frame: WorkspaceTerminalFrame,
    created: Instant,
}
impl CaptureCache {
    pub(super) fn continuation(
        &mut self,
        device: &str,
        request: &WorkspaceTerminalRequest,
    ) -> Result<WorkspaceTerminalUpdate, &'static str> {
        self.expire();
        let key = scope(request)?;
        let id = request.snapshot_id.as_ref().ok_or("invalid_cursor")?;
        let offset = request.offset.ok_or("invalid_cursor")?;
        let entry = self
            .entries
            .iter()
            .find(|entry| &entry.id == id && entry.device == device && entry.scope == key)
            .ok_or("terminal_changed")?;
        Ok(WorkspaceTerminalUpdate {
            version: entry.frame.version.clone(),
            fragment: Some(
                entry
                    .frame
                    .fragment(id, offset)
                    .map_err(|_| "invalid_cursor")?,
            ),
        })
    }
    fn expire(&mut self) {
        self.entries
            .retain(|entry| entry.created.elapsed() < Duration::from_secs(30));
    }
    pub(super) fn update(
        &mut self,
        device: &str,
        request: &WorkspaceTerminalRequest,
        frame: WorkspaceTerminalFrame,
    ) -> Result<WorkspaceTerminalUpdate, &'static str> {
        // 30s allows a bounded 120-row capture to cross the relay budget; at most8 captures in RAM.
        self.expire();
        let key = scope(request)?;
        if request.before.is_none() && request.known_version.as_ref() == Some(&frame.version) {
            return Ok(WorkspaceTerminalUpdate {
                version: frame.version,
                fragment: None,
            });
        }
        let id = crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
        let fragment = frame.fragment(&id, 0).map_err(|_| "terminal_unavailable")?;
        let result = WorkspaceTerminalUpdate {
            version: frame.version.clone(),
            fragment: Some(fragment),
        };
        if result
            .fragment
            .as_ref()
            .is_some_and(|part| part.next_offset.is_some())
        {
            // Replace this reader's previous capture rather than allowing refreshes to evict other phones.
            self.entries
                .retain(|entry| entry.device != device || entry.scope != key);
            if self.entries.len() == 8 {
                self.entries.pop_front();
            }
            self.entries.push_back(Entry {
                id,
                device: device.into(),
                scope: key,
                frame,
                created: Instant::now(),
            });
        }
        Ok(result)
    }
}
fn scope(request: &WorkspaceTerminalRequest) -> Result<String, &'static str> {
    let mut scope = request.clone();
    scope.known_version = None;
    scope.snapshot_id = None;
    scope.offset = None;
    serde_json::to_string(&scope).map_err(|_| "invalid_request")
}

#[cfg(test)]
#[path = "../../tests/unit/workspace_terminal_cache_tests.rs"]
mod tests;
