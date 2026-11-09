//! Atomic, one-time publication of legacy project data into the global brain directory.
//! Call only on a background thread, before starting services or submitting state writes.
mod records;
mod snapshot;
use records::invalid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub struct WorkspaceSource {
    pub workspace_id: String,
    pub root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct MigrationReport {
    pub directory: PathBuf,
    pub reused: bool,
    pub archived_workspaces: Vec<String>,
}

#[derive(Deserialize, Serialize)]
struct Manifest {
    version: u32,
    archived_workspaces: Vec<String>,
}

pub fn workspace_hash(workspace_id: &str) -> String {
    Sha256::digest(workspace_id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn active_dir(base: &Path) -> PathBuf {
    base.join("global/current")
}

pub fn proposal_file(directory: &Path, workspace_id: &str) -> PathBuf {
    directory
        .join("proposals")
        .join(format!("{}.json", workspace_hash(workspace_id)))
}

pub fn migrate(base: &Path, sources: &[WorkspaceSource]) -> io::Result<MigrationReport> {
    let global = base.join("global");
    fs::create_dir_all(&global)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(global.join("migration.lock"))?;
    lock.lock()?;
    let current = active_dir(base);
    if fs::symlink_metadata(&current).is_ok() {
        let target = fs::read_link(&current)?;
        let parts: Vec<_> = target.components().collect();
        if !matches!(parts.as_slice(), [Component::Normal(first), Component::Normal(_)] if *first == "generations")
        {
            return Err(invalid(&current, "invalid global generation target"));
        }
        let manifest_path = current.join("manifest.json");
        let manifest: Manifest = serde_json::from_value(
            records::read_json(&manifest_path)?
                .ok_or_else(|| invalid(&manifest_path, "missing migration manifest"))?,
        )
        .map_err(|error| invalid(&manifest_path, error))?;
        if manifest.version != 1 {
            return Err(invalid(&manifest_path, "unsupported migration version"));
        }
        snapshot::validate(&current)?;
        return Ok(MigrationReport {
            directory: current,
            reused: true,
            archived_workspaces: manifest.archived_workspaces,
        });
    }
    let data = snapshot::collect(base, sources)?;
    let generations = global.join("generations");
    fs::create_dir_all(&generations)?;
    let staged = tempfile::Builder::new()
        .prefix("v1-")
        .tempdir_in(&generations)?;
    data.write(staged.path())?;
    let manifest = Manifest {
        version: 1,
        archived_workspaces: data.archived_workspaces.clone(),
    };
    crate::persistence::atomic_write(
        &staged.path().join("manifest.json"),
        &serde_json::to_vec(&manifest)?,
    )?;
    File::open(staged.path())?.sync_all()?;
    let generation = staged.keep();
    File::open(&generations)?.sync_all()?;
    // A single symlink rename publishes goals, proposals, decisions and settings together.
    let publication = tempfile::Builder::new()
        .prefix("publish-")
        .tempdir_in(&global)?;
    let pointer = publication.path().join("current");
    let target =
        Path::new("generations").join(generation.file_name().expect("generation directory name"));
    std::os::unix::fs::symlink(target, &pointer)?;
    fs::rename(pointer, &current)?;
    File::open(&global)?.sync_all()?;
    Ok(MigrationReport {
        directory: current,
        reused: false,
        archived_workspaces: manifest.archived_workspaces,
    })
}
