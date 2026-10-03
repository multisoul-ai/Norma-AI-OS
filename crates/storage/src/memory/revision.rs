use super::{document, fs::Directory, MemoryError, MemoryResult, MemorySnapshot, MemoryStore};
use agent::{MemoryEntry, MemoryEntryId, MemoryRevision, MemoryScope, MemorySourceReference};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
// Bounded copying keeps a malformed collection from exhausting the host.
const MAX_COLLECTION_BYTES: usize = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_POINTER_BYTES: usize = 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    revision: MemoryRevision,
    manifest_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    scope: MemoryScope,
    revision: MemoryRevision,
    parent: Option<Pointer>,
    catalog_sha256: String,
    entries: BTreeMap<MemoryEntryId, EntryManifest>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryManifest {
    sha256: String,
    sources: Vec<MemorySourceReference>,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn revision_name(revision: MemoryRevision) -> String {
    format!("{:020}", revision.number())
}

impl MemoryStore {
    pub(super) fn load_current(
        &self,
        scope: &MemoryScope,
        directory: &Directory,
    ) -> MemoryResult<MemorySnapshot> {
        let bytes = match directory.read("CURRENT", MAX_POINTER_BYTES) {
            Ok(bytes) => bytes,
            Err(MemoryError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MemorySnapshot {
                    root: self.root.clone(),
                    scope: scope.clone(),
                    revision: MemoryRevision::INITIAL,
                    manifest_sha256: None,
                    catalog: document::catalog(&BTreeMap::new())?,
                    entries: BTreeMap::new(),
                });
            }
            Err(error) => return Err(error),
        };
        let pointer: Pointer = serde_json::from_slice(&bytes)?;
        self.load_revision(scope, directory, &pointer)
    }

    fn load_revision(
        &self,
        scope: &MemoryScope,
        directory: &Directory,
        pointer: &Pointer,
    ) -> MemoryResult<MemorySnapshot> {
        if pointer.revision == MemoryRevision::INITIAL {
            return Err(MemoryError::CorruptRevision);
        }
        let (revision, manifest) = read_manifest(scope, directory, pointer)?;
        let topics = revision.child("topics", false)?;
        let catalog_bytes = revision.read("MEMORY.md", document::MAX_CATALOG_BYTES)?;
        if digest(&catalog_bytes) != manifest.catalog_sha256 {
            return Err(MemoryError::CorruptRevision);
        }
        let catalog = String::from_utf8(catalog_bytes).map_err(|_| MemoryError::CorruptRevision)?;
        let mut entries = BTreeMap::new();
        let mut total_bytes = 0;
        for (id, record) in manifest.entries {
            let bytes = topics.read(&format!("{}.md", id.as_str()), document::MAX_TOPIC_BYTES)?;
            total_bytes += bytes.len();
            if total_bytes > MAX_COLLECTION_BYTES {
                return Err(MemoryError::TooLarge);
            }
            if digest(&bytes) != record.sha256 {
                return Err(MemoryError::CorruptRevision);
            }
            let markdown = String::from_utf8(bytes).map_err(|_| MemoryError::CorruptRevision)?;
            entries.insert(
                id.clone(),
                document::document(id, markdown, record.sources)?,
            );
        }
        if document::catalog(&entries)? != catalog {
            return Err(MemoryError::CorruptRevision);
        }
        Ok(MemorySnapshot {
            root: self.root.clone(),
            scope: scope.clone(),
            revision: pointer.revision,
            manifest_sha256: Some(pointer.manifest_sha256.clone()),
            catalog,
            entries,
        })
    }

    pub(super) fn published_revision(
        &self,
        scope: &MemoryScope,
        directory: &Directory,
        current: &MemorySnapshot,
        target: MemoryRevision,
    ) -> MemoryResult<MemorySnapshot> {
        let mut pointer = current.manifest_sha256.as_ref().map(|digest| Pointer {
            revision: current.revision,
            manifest_sha256: digest.clone(),
        });
        // ponytail: rollback walks the immutable ancestry; add an authenticated
        // history index only when long-lived collections make this measurably slow.
        while let Some(current) = pointer {
            if current.revision == target {
                return self.load_revision(scope, directory, &current);
            }
            if current.revision.number() < target.number() {
                break;
            }
            let (_, manifest) = read_manifest(scope, directory, &current)?;
            pointer = manifest.parent;
        }
        Err(MemoryError::RevisionNotFound)
    }

    pub(super) fn publish(
        &self,
        scope: &MemoryScope,
        directory: &Directory,
        current: &MemorySnapshot,
        entries: BTreeMap<MemoryEntryId, MemoryEntry>,
    ) -> MemoryResult<MemoryRevision> {
        if entries.is_empty() && !current.entries.is_empty() {
            return Err(MemoryError::EmptyCandidate);
        }
        let catalog = document::catalog(&entries)?;
        if entries
            .values()
            .map(|entry| entry.markdown.len())
            .sum::<usize>()
            > MAX_COLLECTION_BYTES
        {
            return Err(MemoryError::TooLarge);
        }
        let revisions = directory.child("revisions", true)?;
        let mut number = current.revision.number();
        let (revision, prepared) = loop {
            number = number
                .checked_add(1)
                .ok_or(MemoryError::RevisionExhausted)?;
            let revision = MemoryRevision::new(number);
            // A crash can leave an unpublished directory; never reuse its identity.
            match revisions.create_child(&revision_name(revision)) {
                Ok(directory) => break (revision, directory),
                Err(MemoryError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    continue
                }
                Err(error) => return Err(error),
            }
        };
        let topics = prepared.child("topics", true)?;
        let mut records = BTreeMap::new();
        // ponytail: copy the bounded collection for each commit; share unchanged
        // immutable files only if real memory volumes make these copies expensive.
        for (id, entry) in entries {
            topics.write_new(&format!("{}.md", id.as_str()), entry.markdown.as_bytes())?;
            records.insert(
                id,
                EntryManifest {
                    sha256: digest(entry.markdown.as_bytes()),
                    sources: entry.sources,
                },
            );
        }
        topics.sync()?;
        prepared.write_new("MEMORY.md", catalog.as_bytes())?;
        let parent = current.manifest_sha256.as_ref().map(|digest| Pointer {
            revision: current.revision,
            manifest_sha256: digest.clone(),
        });
        let manifest = Manifest {
            schema: 1,
            scope: scope.clone(),
            revision,
            parent,
            catalog_sha256: digest(catalog.as_bytes()),
            entries: records,
        };
        let bytes = serde_json::to_vec(&manifest)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(MemoryError::TooLarge);
        }
        prepared.write_new("manifest.json", &bytes)?;
        prepared.sync()?;
        revisions.sync()?;
        let pointer = Pointer {
            revision,
            manifest_sha256: digest(&bytes),
        };
        // Verify exactly what the reader will see before making the pointer visible.
        self.load_revision(scope, directory, &pointer)?;
        // The directory reservation makes this identity unique across restarts;
        // a reused PID or leftover temporary pointer cannot block the next commit.
        let temporary = format!(".CURRENT-{}", revision_name(revision));
        directory.write_new(&temporary, &serde_json::to_vec(&pointer)?)?;
        directory.rename(&temporary, "CURRENT")?;
        match directory.sync() {
            Ok(()) => Ok(revision),
            Err(MemoryError::Io(source)) => {
                Err(MemoryError::PublishedButUnsynced { revision, source })
            }
            Err(error) => Err(error),
        }
    }
}

fn read_manifest(
    scope: &MemoryScope,
    directory: &Directory,
    pointer: &Pointer,
) -> MemoryResult<(Directory, Manifest)> {
    let revision = directory
        .child("revisions", false)?
        .child(&revision_name(pointer.revision), false)?;
    let bytes = revision.read("manifest.json", MAX_MANIFEST_BYTES)?;
    if digest(&bytes) != pointer.manifest_sha256 {
        return Err(MemoryError::CorruptRevision);
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if manifest.schema != 1
        || &manifest.scope != scope
        || manifest.revision != pointer.revision
        || manifest.entries.is_empty()
        || manifest.parent.as_ref().is_some_and(|p| {
            p.revision == MemoryRevision::INITIAL
                || p.revision.number() >= pointer.revision.number()
        })
    {
        return Err(MemoryError::CorruptRevision);
    }
    Ok((revision, manifest))
}
