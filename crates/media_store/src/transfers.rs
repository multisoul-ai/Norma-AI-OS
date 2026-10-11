use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaAvailability {
    Missing,
    Downloading { received: u64, total: u64 },
    Ready(PathBuf),
    Failed(String),
    Cancelled,
}

/// Identity belongs to the transfer, never to a selected wallpaper or guide topic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferId(u64);

#[derive(Default)]
pub struct MediaTransfers {
    serial: u64,
    entries: HashMap<String, (TransferId, MediaAvailability)>,
}

impl MediaTransfers {
    pub fn state(&self, rel: &str) -> MediaAvailability {
        self.entries
            .get(rel)
            .map(|(_, state)| state.clone())
            .unwrap_or(MediaAvailability::Missing)
    }

    pub fn begin(&mut self, rel: &str, bytes: u64, retry: bool) -> Option<TransferId> {
        match self.state(rel) {
            MediaAvailability::Missing => {}
            MediaAvailability::Failed(_)
            | MediaAvailability::Cancelled
            | MediaAvailability::Ready(_)
                if retry => {}
            _ => return None,
        }
        self.serial = self.serial.checked_add(1)?;
        let id = TransferId(self.serial);
        self.entries.insert(
            rel.into(),
            (
                id,
                MediaAvailability::Downloading {
                    received: 0,
                    total: bytes,
                },
            ),
        );
        Some(id)
    }

    pub fn progress(&mut self, rel: &str, id: TransferId, received: u64) -> bool {
        let Some((
            current,
            MediaAvailability::Downloading {
                received: previous,
                total,
            },
        )) = self.entries.get_mut(rel)
        else {
            return false;
        };
        if *current != id || received < *previous {
            return false;
        }
        *previous = received.min(*total);
        true
    }

    pub fn finish(&mut self, rel: &str, id: TransferId, result: Result<PathBuf, String>) -> bool {
        let Some((current, state)) = self.entries.get_mut(rel) else {
            return false;
        };
        if *current != id || !matches!(state, MediaAvailability::Downloading { .. }) {
            return false;
        }
        *state = match result {
            Ok(path) => MediaAvailability::Ready(path),
            Err(error) => MediaAvailability::Failed(error),
        };
        true
    }

    pub fn cancel(&mut self, rel: &str) -> bool {
        let Some((_, state)) = self.entries.get_mut(rel) else {
            return false;
        };
        if !matches!(state, MediaAvailability::Downloading { .. }) {
            return false;
        }
        *state = MediaAvailability::Cancelled;
        true
    }

    /// Used once during background cache discovery, before requests are enabled.
    pub fn seed_verified(&mut self, rel: &str, path: PathBuf) {
        self.entries
            .entry(rel.into())
            .or_insert((TransferId(0), MediaAvailability::Ready(path)));
    }
}
