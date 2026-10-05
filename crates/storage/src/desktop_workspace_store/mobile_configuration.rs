use super::*;
use rusqlite::{OptionalExtension, TransactionBehavior};

impl DesktopWorkspaceStore {
    /// Revalidate captured execution roots; a removed or replaced area cannot dispatch later.
    pub fn mobile_scope_current(
        &self,
        workspace: &str,
        captured: &[DesktopProjectArea],
    ) -> StorageResult<bool> {
        if self.get(workspace)?.is_none() {
            return Ok(false);
        }
        let current = self.list_projects(workspace)?;
        Ok(current.len() == captured.len()
            && captured.iter().all(|area| {
                area.desktop_workspace_id == workspace
                    && current.iter().any(|live| {
                        live.id == area.id
                            && live.root == area.root
                            && live.project_key == area.project_key
                    })
            }))
    }

    pub fn configuration_revision(&self, id: &str) -> StorageResult<u64> {
        let conn = open_app_connection(&self.path)?;
        revision(&conn, id)
    }

    pub fn observe_agent_configuration(&self, id: &str, fingerprint: &str) -> StorageResult<u64> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT fingerprint FROM desktop_configuration_fingerprints WHERE workspace_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if previous.as_deref() != Some(fingerprint) {
            tx.execute("INSERT INTO desktop_configuration_fingerprints VALUES(?1,?2) ON CONFLICT(workspace_id) DO UPDATE SET fingerprint=excluded.fingerprint",params![id,fingerprint])?;
            if previous.is_some() {
                tx.execute("UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=?1",[id])?;
            }
        }
        let value = revision(&tx, id)?;
        tx.commit()?;
        Ok(value)
    }

    pub fn rename_if_revision(&self, id: &str, expected: u64, name: &str) -> StorageResult<()> {
        let name = Self::validate_name(name)?;
        self.edit_at_revision(id, expected, |tx| {
            tx.execute(
                "UPDATE desktop_workspaces SET name=?2 WHERE id=?1",
                params![id, name],
            )?;
            Ok(())
        })
    }

    /// The caller resolves a previously registered project on the Mac. No phone-supplied path.
    pub fn add_project_if_revision(
        &self,
        id: &str,
        expected: u64,
        root: &std::path::Path,
    ) -> StorageResult<()> {
        let project = identify_project(root)?;
        let area = random_workspace_id()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        self.edit_at_revision(id, expected, |tx| {
            tx.execute("INSERT INTO desktop_project_areas(id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(desktop_workspace_id,project_key) DO NOTHING",params![area,id,project.key.to_string_lossy(),project.root.to_string_lossy(),workspace::display_name_for_path(&project.root),now])?;
            Ok(())
        })
    }

    /// Detach the catalog entry, retaining the historical canvas database.
    pub fn remove_project_if_revision(
        &self,
        id: &str,
        expected: u64,
        area: &str,
    ) -> StorageResult<()> {
        self.edit_at_revision(id, expected, |tx| {
            tx.execute("UPDATE desktop_workspaces SET expanded_area_id=NULL WHERE id=?1 AND expanded_area_id=?2",params![id,area])?;
            let removed = tx.execute("DELETE FROM desktop_project_areas WHERE desktop_workspace_id=?1 AND id=?2", params![id,area])?;
            if removed != 1 { return Err(StorageError::InvalidMobileCommand("configuration_changed".into())); }
            Ok(())
        })
    }

    pub fn bump_configuration_if_revision(&self, id: &str, expected: u64) -> StorageResult<()> {
        self.edit_at_revision(id, expected, |tx| {
            tx.execute("UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=?1",[id])?;
            Ok(())
        })
    }

    fn edit_at_revision(
        &self,
        id: &str,
        expected: u64,
        edit: impl FnOnce(&rusqlite::Transaction<'_>) -> StorageResult<()>,
    ) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if revision(&tx, id)? != expected {
            return Err(StorageError::InvalidMobileCommand(
                "configuration_conflict".into(),
            ));
        }
        edit(&tx)?;
        tx.commit()?;
        Ok(())
    }
}

fn revision(conn: &Connection, id: &str) -> StorageResult<u64> {
    conn.query_row(
        "SELECT revision FROM desktop_configuration_revisions WHERE workspace_id=?1",
        [id],
        |r| r.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn migrate(conn: &Connection) -> StorageResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS desktop_configuration_fingerprints(workspace_id TEXT PRIMARY KEY REFERENCES desktop_workspaces(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS desktop_configuration_revisions(workspace_id TEXT PRIMARY KEY REFERENCES desktop_workspaces(id) ON DELETE CASCADE,revision INTEGER NOT NULL);

        CREATE TRIGGER IF NOT EXISTS desktop_chat_workspace_created AFTER INSERT ON desktop_workspaces BEGIN INSERT INTO desktop_configuration_revisions VALUES(NEW.id,1); END;
        CREATE TRIGGER IF NOT EXISTS desktop_chat_workspace_renamed AFTER UPDATE OF name ON desktop_workspaces WHEN OLD.name != NEW.name BEGIN UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=NEW.id; END;
        CREATE TRIGGER IF NOT EXISTS desktop_chat_project_added AFTER INSERT ON desktop_project_areas BEGIN UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=NEW.desktop_workspace_id; END;
        CREATE TRIGGER IF NOT EXISTS desktop_chat_project_removed AFTER DELETE ON desktop_project_areas BEGIN UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=OLD.desktop_workspace_id; END;
        CREATE TRIGGER IF NOT EXISTS desktop_chat_project_edited AFTER UPDATE OF root_path,project_key,display_name ON desktop_project_areas BEGIN UPDATE desktop_configuration_revisions SET revision=revision+1 WHERE workspace_id=NEW.desktop_workspace_id; END;")?;
    // Even INSERT OR IGNORE takes a write lock when no row changes. Existing-schema reads
    // must stay read-only while the event ledger is being written by another connection.
    let missing: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM desktop_workspaces w WHERE NOT EXISTS(SELECT 1 FROM desktop_configuration_revisions r WHERE r.workspace_id=w.id))",[],|r|r.get(0))?;
    if missing {
        conn.execute("INSERT OR IGNORE INTO desktop_configuration_revisions SELECT id,1 FROM desktop_workspaces",[])?;
    }
    Ok(())
}
