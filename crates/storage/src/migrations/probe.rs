use crate::StorageResult;
use rusqlite::Connection;

// Temporary failure-only diagnostics: retain errors and remove after locating the statement.
pub(super) fn step<F>(connection: &Connection, run: F) -> StorageResult<()>
where
    F: FnOnce(&Connection) -> StorageResult<()>,
{
    let stage = std::any::type_name::<F>();
    let started = std::time::Instant::now();
    run(connection).inspect_err(|error| {
        eprintln!(
            "[DEBUG-storage15] migration stage={stage} thread={:?} path={:?} elapsed_ms={} autocommit={} error={error:?}",
            std::thread::current().id(), connection.path(), started.elapsed().as_millis(), connection.is_autocommit()
        );
    })
}
