use super::FeatureGuideState;
use storage::SettingsStore;

pub(crate) const GUIDE_STATUS_KEY: &str = "feature_guide.status.v1";
pub(crate) const GUIDE_TOPIC_KEY: &str = "feature_guide.topic.v1";

type PendingFeatureGuideWrite = (SettingsStore, Vec<(String, Option<String>)>);

pub(crate) struct FeatureGuideSettingsState {
    storage: Option<SettingsStore>,
    pending: Option<FeatureGuideState>,
    persistence_running: bool,
}

impl FeatureGuideSettingsState {
    pub(crate) fn load_from_store(
        store: SettingsStore,
        had_recent_workspaces: bool,
    ) -> (FeatureGuideState, Self) {
        let status = load_string(&store, GUIDE_STATUS_KEY);
        let topic = load_string(&store, GUIDE_TOPIC_KEY);
        (
            FeatureGuideState::load(status.as_deref(), topic.as_deref(), had_recent_workspaces),
            Self {
                storage: Some(store),
                pending: None,
                persistence_running: false,
            },
        )
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn in_memory_suppressed() -> (FeatureGuideState, Self) {
        (
            FeatureGuideState::load(Some("suppressed"), None, true),
            Self {
                storage: None,
                pending: None,
                persistence_running: false,
            },
        )
    }

    pub(crate) fn queue(&mut self, state: FeatureGuideState) {
        self.pending = Some(state);
    }

    pub(crate) fn take_pending_write(&mut self) -> Option<PendingFeatureGuideWrite> {
        if self.persistence_running {
            return None;
        }
        let state = self.pending.take()?;
        let store = self.storage.clone()?;
        self.persistence_running = true;
        Some((
            store,
            vec![
                (
                    GUIDE_STATUS_KEY.to_string(),
                    Some(state.serialized_status().to_string()),
                ),
                (
                    GUIDE_TOPIC_KEY.to_string(),
                    Some(state.serialized_topic().to_string()),
                ),
            ],
        ))
    }

    pub(crate) fn finish_write(&mut self) {
        self.persistence_running = false;
    }

    #[cfg(test)]
    pub(crate) fn persistence_running(&self) -> bool {
        self.persistence_running
    }
}

fn load_string(store: &SettingsStore, key: &str) -> Option<String> {
    match store.get_string(key) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, key, "failed to load feature guide setting");
            None
        }
    }
}
