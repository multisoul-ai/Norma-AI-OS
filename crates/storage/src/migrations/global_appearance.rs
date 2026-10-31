//! Adopt the last visible desktop appearance once, retaining legacy rows for compatibility.
use crate::{DispatchEffectPreference, StorageResult, WallpaperReference};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

const MARKER: &str = "appearance.global_migrated.v1";

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    if migrated(connection)? {
        return Ok(());
    }
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    if migrated(&transaction)? {
        return transaction.commit().map_err(Into::into);
    }
    let latest = transaction.query_row(
        "SELECT a.wallpaper_json, w.wallpaper_json, a.dispatch_effect_json, w.dispatch_effect_json
         FROM desktop_workspaces w LEFT JOIN desktop_project_areas a
         ON a.desktop_workspace_id=w.id AND a.id=w.expanded_area_id
         ORDER BY w.last_opened_at_unix_ms DESC, w.id LIMIT 1",
        [],
        |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?,
                  row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?)),
    ).optional()?;
    if let Some((project_wallpaper, workspace_wallpaper, project_effect, workspace_effect)) = latest
    {
        let legacy = crate::desktop_workspace_store::wallpaper::legacy_reference(&transaction)?;
        let candidates: Vec<_> = [project_wallpaper, workspace_wallpaper]
            .into_iter()
            .flatten()
            .filter_map(|json| serde_json::from_str::<WallpaperReference>(&json).ok())
            .chain(legacy)
            .filter(|reference| reference.validate().is_ok())
            .collect();
        if !candidates.is_empty() {
            set(
                &transaction,
                "wallpaper.global_candidates.v1",
                &serde_json::to_string(&candidates)?,
            )?;
        }
        let wallpaper = candidates.into_iter().next();
        if let Some(wallpaper) = wallpaper {
            let (kind, value) = match wallpaper {
                WallpaperReference::Builtin { rel } => ("builtin", rel),
                WallpaperReference::Custom { filename } => ("custom", filename),
            };
            set(&transaction, "wallpaper.kind", kind)?;
            set(&transaction, "wallpaper.value", &value)?;
        }
        let effect = [project_effect, workspace_effect]
            .into_iter()
            .flatten()
            .filter_map(|json| serde_json::from_str::<DispatchEffectPreference>(&json).ok())
            .find(|preference| preference.validate().is_ok());
        if let Some(effect) = effect {
            transaction.execute(
                "INSERT OR IGNORE INTO settings(key,value,updated_at_unix) VALUES('effects.dispatch.v1',?1,0)",
                [serde_json::to_string(&effect)?],
            )?;
        }
    }
    set(&transaction, MARKER, "1")?;
    transaction.commit()?;
    Ok(())
}

fn migrated(connection: &Connection) -> StorageResult<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?1)",
        [MARKER],
        |row| row.get(0),
    )?)
}

fn set(connection: &Connection, key: &str, value: &str) -> StorageResult<()> {
    connection.execute(
        "INSERT INTO settings(key,value,updated_at_unix) VALUES(?1,?2,0)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    )?;
    Ok(())
}
