use audio::PcmAudioChunk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const WORKSPACE_CACHE_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Clone)]
pub(super) struct CachedAnnouncementSegments {
    pub(super) name: Arc<PcmAudioChunk>,
    pub(super) outcome: Arc<PcmAudioChunk>,
}

#[derive(Clone, Default)]
pub(super) struct CachedAnnouncementSegmentLookup {
    pub(super) name: Option<Arc<PcmAudioChunk>>,
    pub(super) outcome: Option<Arc<PcmAudioChunk>>,
}

#[derive(Default)]
pub(crate) struct AgentAnnouncementAudioCache {
    workspaces: HashMap<PathBuf, WorkspaceAnnouncementAudioCache>,
}

struct WorkspaceAnnouncementAudioCache {
    segments: HashMap<AnnouncementSegmentKey, Arc<PcmAudioChunk>>,
    last_accessed_at: SystemTime,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum AnnouncementSegmentRole {
    AgentName,
    Outcome,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AnnouncementSegmentKey {
    role: AnnouncementSegmentRole,
    text: String,
}

impl AnnouncementSegmentKey {
    fn agent_name(text: &str) -> Self {
        Self {
            role: AnnouncementSegmentRole::AgentName,
            text: text.to_string(),
        }
    }

    fn outcome(text: &str) -> Self {
        Self {
            role: AnnouncementSegmentRole::Outcome,
            text: text.to_string(),
        }
    }
}

impl AgentAnnouncementAudioCache {
    pub(super) fn clear_for_voice_change(&mut self) {
        self.workspaces.clear();
    }

    #[cfg(test)]
    pub(super) fn lookup(
        &mut self,
        workspace_root: &Path,
        agent_name: &str,
        outcome_word: &str,
        now: SystemTime,
    ) -> Option<CachedAnnouncementSegments> {
        let segments = self.lookup_segments(workspace_root, agent_name, outcome_word, now);
        Some(CachedAnnouncementSegments {
            name: segments.name?,
            outcome: segments.outcome?,
        })
    }

    pub(super) fn lookup_segments(
        &mut self,
        workspace_root: &Path,
        agent_name: &str,
        outcome_word: &str,
        now: SystemTime,
    ) -> CachedAnnouncementSegmentLookup {
        self.prune_expired_workspaces(now);
        let Some(workspace) = self.workspaces.get_mut(workspace_root) else {
            return CachedAnnouncementSegmentLookup::default();
        };
        workspace.last_accessed_at = now;
        CachedAnnouncementSegmentLookup {
            name: workspace
                .segments
                .get(&AnnouncementSegmentKey::agent_name(agent_name))
                .cloned(),
            outcome: workspace
                .segments
                .get(&AnnouncementSegmentKey::outcome(outcome_word))
                .cloned(),
        }
    }

    #[cfg(test)]
    pub(super) fn insert(
        &mut self,
        workspace_root: &Path,
        agent_name: &str,
        outcome_word: &str,
        name: PcmAudioChunk,
        outcome: PcmAudioChunk,
        now: SystemTime,
    ) {
        self.insert_cached_segments(
            workspace_root,
            agent_name,
            outcome_word,
            CachedAnnouncementSegments {
                name: Arc::new(name),
                outcome: Arc::new(outcome),
            },
            now,
        );
    }

    pub(super) fn insert_cached_segments(
        &mut self,
        workspace_root: &Path,
        agent_name: &str,
        outcome_word: &str,
        segments: CachedAnnouncementSegments,
        now: SystemTime,
    ) {
        self.prune_expired_workspaces(now);
        let workspace = self
            .workspaces
            .entry(workspace_root.to_path_buf())
            .or_insert_with(|| WorkspaceAnnouncementAudioCache {
                segments: HashMap::new(),
                last_accessed_at: now,
            });
        workspace.last_accessed_at = now;
        workspace.segments.insert(
            AnnouncementSegmentKey::agent_name(agent_name),
            segments.name,
        );
        workspace.segments.insert(
            AnnouncementSegmentKey::outcome(outcome_word),
            segments.outcome,
        );
    }

    pub(super) fn prune_expired_workspaces(&mut self, now: SystemTime) {
        self.workspaces.retain(|_, workspace| {
            now.duration_since(workspace.last_accessed_at)
                .map_or(true, |elapsed| elapsed < WORKSPACE_CACHE_RETENTION)
        });
    }

    #[cfg(test)]
    fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_announcement_cache_tests.rs"]
mod tests;
