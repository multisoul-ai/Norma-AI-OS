use std::path::{Component, Path, PathBuf};
#[cfg(any(test, feature = "e2e-test-support"))]
use std::sync::atomic::{AtomicU64, Ordering};
use storage::{DatabasePaths, StorageResult};

#[cfg(any(test, feature = "e2e-test-support"))]
const AUTOMATION_DATABASE_ROOT_NAME: &str = "norma-os-database-tests";
const EXTERNAL_TEST_DATA_ROOT_ENV: &str = "NORMA_TEST_DATA_ROOT";
#[cfg(any(test, feature = "e2e-test-support"))]
static AUTOMATION_DATABASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TestDatabaseGuard {
    root: PathBuf,
    parent: PathBuf,
}

impl Drop for TestDatabaseGuard {
    fn drop(&mut self) {
        if self.root.parent() != Some(self.parent.as_path()) {
            tracing::error!(
                path = %self.root.display(),
                "refusing to remove test database directory outside its expected parent"
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %self.root.display(),
                    %error,
                    "failed to remove test database directory"
                );
            }
        }
    }
}

pub(crate) fn default_database_paths() -> StorageResult<DatabasePaths> {
    let home = dirs::home_dir().ok_or(storage::StorageError::MissingHomeDirectory)?;
    let production_data = cfg!(feature = "production-data");
    let test_root = (!production_data)
        .then(|| std::env::var_os(EXTERNAL_TEST_DATA_ROOT_ENV))
        .flatten()
        .filter(|root| !root.is_empty())
        .map(PathBuf::from);
    database_paths_for_home_and_test_root(&home, production_data, test_root.as_deref())
}

pub(crate) fn default_database_context() -> StorageResult<(DatabasePaths, storage::AppStore)> {
    let paths = default_database_paths()?;
    let store = storage::AppStore::initialize(&paths)?;
    Ok((paths, store))
}

#[cfg(test)]
pub(crate) fn database_paths_for_home(home: &Path, production_data: bool) -> DatabasePaths {
    if production_data {
        DatabasePaths::production_for_home(home)
    } else {
        DatabasePaths::local_test_for_home(home)
    }
}

pub(crate) fn database_paths_for_home_and_test_root(
    home: &Path,
    production_data: bool,
    test_root: Option<&Path>,
) -> StorageResult<DatabasePaths> {
    if production_data {
        return Ok(DatabasePaths::production_for_home(home));
    }
    match test_root {
        Some(root) if root.is_absolute() => {
            let candidate = DatabasePaths::isolated_test(root);
            let production = DatabasePaths::production_for_home(home);
            if paths_resolve_to_same_file(candidate.app_db_path(), production.app_db_path()) {
                return Err(storage::StorageError::InvalidTestDataRoot(
                    root.to_path_buf(),
                ));
            }
            Ok(candidate)
        }
        Some(root) => Err(storage::StorageError::InvalidTestDataRoot(
            root.to_path_buf(),
        )),
        None => Ok(DatabasePaths::local_test_for_home(home)),
    }
}

fn paths_resolve_to_same_file(left: &Path, right: &Path) -> bool {
    let left = resolve_existing_ancestors(left);
    let right = resolve_existing_ancestors(right);
    if left == right {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn resolve_existing_ancestors(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        let Ok(resolved_ancestor) = std::fs::canonicalize(ancestor) else {
            continue;
        };
        let suffix = path
            .strip_prefix(ancestor)
            .expect("ancestor iteration must yield path prefixes");
        return normalize_absolute_path(&resolved_ancestor.join(suffix));
    }
    normalize_absolute_path(path)
}

fn normalize_absolute_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) fn isolated_test_database() -> StorageResult<(DatabasePaths, TestDatabaseGuard)> {
    let parent = std::env::temp_dir().join(AUTOMATION_DATABASE_ROOT_NAME);
    std::fs::create_dir_all(&parent)?;
    let root = loop {
        let sequence = AUTOMATION_DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!("{}-{sequence}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let paths = DatabasePaths::isolated_test(&root);
    Ok((paths, TestDatabaseGuard { root, parent }))
}
