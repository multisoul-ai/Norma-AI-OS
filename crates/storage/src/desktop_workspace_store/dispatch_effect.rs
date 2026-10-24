use super::DesktopWorkspaceStore;
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_ID_LEN: usize = 32;

/// 派发特效选择：只存字符串 id，款式枚举与绘制归 app；未知 id 原样保留以便旧版本读新数据。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DispatchEffectPreference {
    pub style: String,
    /// 按派发来源的覆盖，`None` 表示跟随 `style`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idea: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<String>,
}

impl DispatchEffectPreference {
    pub fn validate(&self) -> StorageResult<()> {
        let valid = valid_id(&self.style)
            && [&self.voice, &self.idea, &self.handoff]
                .into_iter()
                .flatten()
                .all(|id| valid_id(id));
        if valid {
            Ok(())
        } else {
            Err(StorageError::InvalidDispatchEffect)
        }
    }
}

/// 启动与保存后一次性读出，供 app 缓存；渲染与派发路径只读缓存。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DispatchEffectSnapshot {
    pub workspaces: Vec<(String, DispatchEffectPreference)>,
    /// `(workspace_id, area_id, preference)`
    pub projects: Vec<(String, String, DispatchEffectPreference)>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// 损坏或不合法的数据一律当作“跟随”，设置页不能因脏数据崩溃。
fn decode(value: Option<String>) -> Option<DispatchEffectPreference> {
    let pref: DispatchEffectPreference = serde_json::from_str(&value?).ok()?;
    pref.validate().ok()?;
    Some(pref)
}

fn require_changed(changed: usize) -> StorageResult<()> {
    if changed == 0 {
        Err(rusqlite::Error::QueryReturnedNoRows.into())
    } else {
        Ok(())
    }
}

impl DesktopWorkspaceStore {
    pub fn list_dispatch_effects(&self) -> StorageResult<DispatchEffectSnapshot> {
        let connection = open_app_connection(&self.path)?;
        let mut workspaces = Vec::new();
        let mut statement = connection.prepare(
            "SELECT id, dispatch_effect_json FROM desktop_workspaces
             WHERE dispatch_effect_json IS NOT NULL ORDER BY id",
        )?;
        for row in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))? {
            let (id, json) = row?;
            if let Some(pref) = decode(json) {
                workspaces.push((id, pref));
            }
        }
        let mut projects = Vec::new();
        let mut statement = connection.prepare(
            "SELECT desktop_workspace_id, id, dispatch_effect_json FROM desktop_project_areas
             WHERE dispatch_effect_json IS NOT NULL ORDER BY desktop_workspace_id, id",
        )?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get(2)?,
            ))
        })? {
            let (workspace_id, area_id, json) = row?;
            if let Some(pref) = decode(json) {
                projects.push((workspace_id, area_id, pref));
            }
        }
        Ok(DispatchEffectSnapshot {
            workspaces,
            projects,
        })
    }

    pub fn workspace_dispatch_effect(
        &self,
        workspace_id: &str,
    ) -> StorageResult<Option<DispatchEffectPreference>> {
        let connection = open_app_connection(&self.path)?;
        let value = connection
            .query_row(
                "SELECT dispatch_effect_json FROM desktop_workspaces WHERE id=?1",
                [workspace_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        Ok(decode(value))
    }

    pub fn set_workspace_dispatch_effect(
        &self,
        workspace_id: &str,
        preference: &DispatchEffectPreference,
    ) -> StorageResult<()> {
        preference.validate()?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET dispatch_effect_json=?2 WHERE id=?1",
            params![workspace_id, serde_json::to_string(preference)?],
        )?;
        require_changed(changed)
    }

    /// 回到“未设置”（使用各来源的默认款式）。
    pub fn clear_workspace_dispatch_effect(&self, workspace_id: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET dispatch_effect_json=NULL WHERE id=?1",
            [workspace_id],
        )?;
        require_changed(changed)
    }

    pub fn project_dispatch_effect(
        &self,
        workspace_id: &str,
        area_id: &str,
    ) -> StorageResult<Option<DispatchEffectPreference>> {
        let connection = open_app_connection(&self.path)?;
        let value = connection
            .query_row(
                "SELECT dispatch_effect_json FROM desktop_project_areas
                 WHERE desktop_workspace_id=?1 AND id=?2",
                params![workspace_id, area_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        Ok(decode(value))
    }

    /// `None` 取消项目覆盖，回到跟随工作空间。
    pub fn set_project_dispatch_effect(
        &self,
        workspace_id: &str,
        area_id: &str,
        preference: Option<&DispatchEffectPreference>,
    ) -> StorageResult<()> {
        if let Some(preference) = preference {
            preference.validate()?;
        }
        let value = preference.map(serde_json::to_string).transpose()?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_project_areas SET dispatch_effect_json=?3
             WHERE desktop_workspace_id=?1 AND id=?2",
            params![workspace_id, area_id, value],
        )?;
        require_changed(changed)
    }
}
