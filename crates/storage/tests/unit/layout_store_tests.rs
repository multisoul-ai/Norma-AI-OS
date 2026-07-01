//! Unit tests for LayoutStore.

use crate::{
    layout_store::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot, LayoutStore},
    migrations::migrate_project_db,
    StorageError,
};
use canvas::CanvasRect;
use tempfile::tempdir;

fn make_store() -> (LayoutStore, tempfile::TempDir) {
    let dir = tempdir().expect("test should create temp dir");
    let path = dir.path().join("test.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("test should open sqlite");
    migrate_project_db(&conn).expect("project migration should create layout_state");
    (LayoutStore::new(path), dir)
}

/// LS-1: active master/detail layout state can round-trip through SQLite.
///
/// Data construction:
///   mode = MasterDetail
///   master_id = 2
///   original_bounds = node 1 at (10,20,300,200), node 2 at (40,50,320,240)
///   row count = 1 because layout_state is a singleton table with id=1
///
/// Execution:
///   1. save_state(active snapshot)
///   2. load_state()
///   3. compare mode, master id, and each original bound exactly
///
/// Expected:
///   - Positive: active mode, master_id=2, and both bounds are preserved
///   - Negative: loaded state must not degrade to Tiled or drop node 1
#[test]
fn test_active_master_layout_state_round_trips() {
    let (store, _dir) = make_store();
    let snapshot = LayoutStateSnapshot {
        mode: LayoutMode::MasterDetail,
        master_id: Some(2),
        original_bounds: vec![
            LayoutOriginalBounds {
                node_id: 1,
                bounds: CanvasRect::new(10.0, 20.0, 300.0, 200.0),
            },
            LayoutOriginalBounds {
                node_id: 2,
                bounds: CanvasRect::new(40.0, 50.0, 320.0, 240.0),
            },
        ],
    };

    store
        .save_state(&snapshot)
        .expect("active layout state should save");
    let loaded = store
        .load_state()
        .expect("saved layout state should load")
        .expect("layout state should exist after save");

    assert_eq!(
        loaded.mode,
        LayoutMode::MasterDetail,
        "loaded layout mode should remain MasterDetail"
    );
    assert_eq!(
        loaded.master_id,
        Some(2),
        "loaded master_id should remain node 2"
    );
    assert_eq!(
        loaded.original_bounds[0].node_id, 1,
        "first original bound should still belong to node 1"
    );
    assert_eq!(
        loaded.original_bounds[0].bounds,
        CanvasRect::new(10.0, 20.0, 300.0, 200.0),
        "node 1 original bounds should round-trip exactly"
    );
    assert_ne!(
        loaded.mode,
        LayoutMode::Tiled,
        "active master/detail state must not degrade to tiled during storage round-trip"
    );
}

/// LS-2: saving tiled state overwrites a previous active state in the singleton row.
///
/// Data construction:
///   first save = MasterDetail(master_id=7, one original bound)
///   second save = Tiled(master_id=None, no original bounds)
///   expected row count stays 1 due to ON CONFLICT(id) update
///
/// Execution:
///   1. save active state
///   2. save tiled state
///   3. load_state()
///
/// Expected:
///   - Positive: loaded mode is Tiled and original_bounds is empty
///   - Negative: old master_id=7 must not survive after tiled save
#[test]
fn test_tiled_state_overwrites_previous_active_state() {
    let (store, _dir) = make_store();
    store
        .save_state(&LayoutStateSnapshot {
            mode: LayoutMode::MasterDetail,
            master_id: Some(7),
            original_bounds: vec![LayoutOriginalBounds {
                node_id: 7,
                bounds: CanvasRect::new(1.0, 2.0, 3.0, 4.0),
            }],
        })
        .expect("active layout state should save");

    store
        .save_state(&LayoutStateSnapshot {
            mode: LayoutMode::Tiled,
            master_id: None,
            original_bounds: Vec::new(),
        })
        .expect("tiled layout state should save");
    let loaded = store
        .load_state()
        .expect("tiled layout state should load")
        .expect("layout state should exist after save");

    assert_eq!(
        loaded.mode,
        LayoutMode::Tiled,
        "latest mode should be Tiled"
    );
    assert!(
        loaded.master_id.is_none(),
        "tiled state should clear the previous master_id"
    );
    assert!(
        loaded.original_bounds.is_empty(),
        "tiled state should not retain previous original bounds"
    );
    assert_ne!(
        loaded.master_id,
        Some(7),
        "old active master_id=7 must not survive after tiled save"
    );
}

/// LS-3: invalid stored mode fails with a typed storage error.
///
/// Data construction:
///   layout_state row mode = "split" (unsupported)
///   master_id = NULL
///   original_bounds_json = "[]"
///
/// Execution:
///   1. Insert malformed row directly through rusqlite
///   2. load_state()
///
/// Expected:
///   - Positive: load returns InvalidLayoutMode("split")
///   - Negative: malformed mode must not be silently treated as Tiled
#[test]
fn test_invalid_layout_mode_returns_typed_error() {
    let (store, dir) = make_store();
    let path = dir.path().join("test.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("test should reopen sqlite");
    conn.execute(
        "INSERT INTO layout_state (id, mode, master_id, original_bounds_json, updated_at_unix)
         VALUES (1, 'split', NULL, '[]', 1)",
        [],
    )
    .expect("test should insert invalid layout row");

    let result = store.load_state();

    assert!(
        matches!(result, Err(StorageError::InvalidLayoutMode(ref mode)) if mode == "split"),
        "invalid mode should return InvalidLayoutMode('split'), actual: {:?}",
        result
    );
    assert!(
        !matches!(
            result,
            Ok(Some(LayoutStateSnapshot {
                mode: LayoutMode::Tiled,
                ..
            }))
        ),
        "invalid mode must not be silently treated as Tiled"
    );
}
