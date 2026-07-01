use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use canvas::{CanvasRect, NodeId};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutMode {
    Tiled,
    MasterDetail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutOriginalBounds {
    pub node_id: NodeId,
    pub bounds: CanvasRect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStateSnapshot {
    pub mode: LayoutMode,
    pub master_id: Option<NodeId>,
    pub original_bounds: Vec<LayoutOriginalBounds>,
}

#[derive(Clone, Debug)]
pub struct LayoutStore {
    path: PathBuf,
}

impl LayoutStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn save_state(&self, snapshot: &LayoutStateSnapshot) -> StorageResult<()> {
        let connection = open_project_connection(&self.path)?;
        let original_bounds_json = serde_json::to_string(&snapshot.original_bounds)?;
        connection.execute(
            r#"
            INSERT INTO layout_state (id, mode, master_id, original_bounds_json, updated_at_unix)
            VALUES (1, ?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                mode = excluded.mode,
                master_id = excluded.master_id,
                original_bounds_json = excluded.original_bounds_json,
                updated_at_unix = excluded.updated_at_unix
            "#,
            params![
                encode_mode(&snapshot.mode),
                snapshot.master_id,
                original_bounds_json,
                current_unix_timestamp()?,
            ],
        )?;
        Ok(())
    }

    pub fn load_state(&self) -> StorageResult<Option<LayoutStateSnapshot>> {
        let connection = open_project_connection(&self.path)?;
        let row = connection
            .query_row(
                "SELECT mode, master_id, original_bounds_json FROM layout_state WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<NodeId>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(mode, master_id, original_bounds_json)| {
            Ok(LayoutStateSnapshot {
                mode: decode_mode(&mode)?,
                master_id,
                original_bounds: serde_json::from_str(&original_bounds_json)?,
            })
        })
        .transpose()
    }
}

fn encode_mode(mode: &LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Tiled => "tiled",
        LayoutMode::MasterDetail => "master_detail",
    }
}

fn decode_mode(mode: &str) -> StorageResult<LayoutMode> {
    match mode {
        "tiled" => Ok(LayoutMode::Tiled),
        "master_detail" => Ok(LayoutMode::MasterDetail),
        other => Err(StorageError::InvalidLayoutMode(other.to_string())),
    }
}

#[cfg(test)]
#[path = "../tests/unit/layout_store_tests.rs"]
mod tests;
