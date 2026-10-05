use super::{select, DesktopWorkspace, DesktopWorkspaceStore};
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Portable asset reference; resolution and builtin registry belong to the app.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WallpaperReference {
    Builtin { rel: String },
    Custom { filename: String },
}

impl WallpaperReference {
    pub fn validate(&self) -> StorageResult<()> {
        let valid = match self {
            Self::Builtin { rel } => {
                rel.starts_with("wallpapers/")
                    && !rel.split('/').any(|part| part == ".." || part.is_empty())
                    && !rel.contains('\\')
                    && !rel.chars().any(char::is_control)
            }
            Self::Custom { filename } => {
                !filename.trim().is_empty()
                    && filename != "."
                    && filename != ".."
                    && !filename.contains(['/', '\\'])
                    && !filename.chars().any(char::is_control)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(StorageError::InvalidWallpaperReference)
        }
    }
}

pub(super) fn decode(value: Option<String>) -> Option<WallpaperReference> {
    let reference: WallpaperReference = serde_json::from_str(&value?).ok()?;
    reference.validate().ok()?;
    Some(reference)
}

pub(crate) fn legacy_reference(
    connection: &Connection,
) -> StorageResult<Option<WallpaperReference>> {
    let read = |key| {
        connection
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()
    };
    let reference = match (read("wallpaper.kind")?, read("wallpaper.value")?) {
        (Some(kind), Some(value)) if kind == "builtin" => {
            Some(WallpaperReference::Builtin { rel: value })
        }
        (Some(kind), Some(value)) if kind == "custom" => {
            Some(WallpaperReference::Custom { filename: value })
        }
        _ => None,
    };
    Ok(reference.filter(|reference| reference.validate().is_ok()))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WallpaperUsage {
    pub workspaces: Vec<String>,
    pub projects: Vec<String>,
}

impl DesktopWorkspaceStore {
    pub fn create_with_wallpaper(
        &self,
        name: &str,
        wallpaper: &WallpaperReference,
    ) -> StorageResult<DesktopWorkspace> {
        wallpaper.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let created = self.create_on_connection(&transaction, name, Some(wallpaper))?;
        transaction.commit()?;
        Ok(created)
    }

    pub fn set_workspace_wallpaper(
        &self,
        id: &str,
        wallpaper: &WallpaperReference,
    ) -> StorageResult<()> {
        wallpaper.validate()?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET wallpaper_json=?2 WHERE id=?1",
            params![id, serde_json::to_string(wallpaper)?],
        )?;
        require_changed(changed)
    }

    pub fn set_project_wallpaper(
        &self,
        workspace_id: &str,
        area_id: &str,
        wallpaper: Option<&WallpaperReference>,
    ) -> StorageResult<()> {
        if let Some(reference) = wallpaper {
            reference.validate()?;
        }
        let value = wallpaper.map(serde_json::to_string).transpose()?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute("UPDATE desktop_project_areas SET wallpaper_json=?3 WHERE desktop_workspace_id=?1 AND id=?2", params![workspace_id, area_id, value])?;
        require_changed(changed)
    }

    pub fn wallpaper_usage(&self, filename: &str) -> StorageResult<WallpaperUsage> {
        let reference = WallpaperReference::Custom {
            filename: filename.into(),
        };
        reference.validate()?;
        let mut usage = WallpaperUsage::default();
        for workspace in self.list()? {
            if workspace.wallpaper.as_ref() == Some(&reference) {
                usage.workspaces.push(workspace.name.clone());
            }
            for area in self.list_projects(&workspace.id)? {
                if area.wallpaper.as_ref() == Some(&reference) {
                    usage
                        .projects
                        .push(format!("{} / {}", workspace.name, area.display_name));
                }
            }
        }
        Ok(usage)
    }

    /// Clear references before the caller removes the shared file. Filesystem failure
    /// leaves an unused asset available for retry, never a broken active reference.
    pub fn clear_custom_wallpaper(
        &self,
        filename: &str,
        default: &WallpaperReference,
    ) -> StorageResult<()> {
        default.validate()?;
        WallpaperReference::Custom {
            filename: filename.into(),
        }
        .validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        transaction.execute("UPDATE desktop_workspaces SET wallpaper_json=?2 WHERE json_valid(wallpaper_json) AND json_extract(wallpaper_json,'$.kind')='custom' AND json_extract(wallpaper_json,'$.filename')=?1", params![filename, serde_json::to_string(default)?])?;
        transaction.execute("UPDATE desktop_project_areas SET wallpaper_json=NULL WHERE json_valid(wallpaper_json) AND json_extract(wallpaper_json,'$.kind')='custom' AND json_extract(wallpaper_json,'$.filename')=?1", [filename])?;
        let legacy = legacy_reference(&transaction)?;
        if legacy
            == Some(WallpaperReference::Custom {
                filename: filename.into(),
            })
        {
            if let WallpaperReference::Builtin { rel } = default {
                transaction.execute(
                    "UPDATE settings SET value='builtin' WHERE key='wallpaper.kind'",
                    [],
                )?;
                transaction.execute(
                    "UPDATE settings SET value=?1 WHERE key='wallpaper.value'",
                    [rel],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn create_on_connection(
        &self,
        connection: &Connection,
        name: &str,
        wallpaper: Option<&WallpaperReference>,
    ) -> StorageResult<DesktopWorkspace> {
        let name = Self::validate_name(name)?;
        let id = crate::workspace_identity_store::random_workspace_id()?;
        let now = super::next_opened_at(connection)?;
        connection.execute("INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms,wallpaper_json) VALUES(?1,?2,?3,?3,?4)", params![id, name, now, wallpaper.map(serde_json::to_string).transpose()?])?;
        select(connection, &id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }
}

fn require_changed(changed: usize) -> StorageResult<()> {
    if changed == 0 {
        Err(rusqlite::Error::QueryReturnedNoRows.into())
    } else {
        Ok(())
    }
}
