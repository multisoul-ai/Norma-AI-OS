//! 幕僚活动折叠行用的预览，以及“上次打开之后的新动作”计数。

use crate::{
    brain_activity_store::BrainTurnUpsert, migrations::open_app_connection, BrainActivityStore,
    StorageResult,
};
use rusqlite::{params, OptionalExtension};

/// 摘要最多这么多个字符，与面板单行显示的需要对齐。
const SUMMARY_CHARS: i64 = 300;
const SEEN_KEY: &str = "actions_seen_at_ms";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainTurnPreview {
    pub launch_id: i64,
    pub area_id: String,
    pub lane: String,
    pub turn: i64,
    /// 各种类的条数，按该种类在本轮首次出现的顺序。
    pub counts: Vec<(String, usize)>,
    /// 最后一条非空回复；没有回复时取提示。
    pub summary: String,
}

impl BrainActivityStore {
    /// 给一页轮次取预览。每页最多百来条，逐轮查询足够快。
    pub fn turn_previews(&self, turns: &[BrainTurnUpsert]) -> StorageResult<Vec<BrainTurnPreview>> {
        if turns.is_empty() {
            return Ok(Vec::new());
        }
        let connection = open_app_connection(&self.path)?;
        let mut counts = connection.prepare(
            "SELECT kind, COUNT(*) FROM brain_trace_items
             WHERE launch_id = ?1 AND area_id = ?2 AND lane = ?3 AND turn = ?4
             GROUP BY kind ORDER BY MIN(id)",
        )?;
        let mut summary = connection.prepare(&format!(
            "SELECT substr(text, 1, {SUMMARY_CHARS}) FROM brain_trace_items
             WHERE launch_id = ?1 AND area_id = ?2 AND lane = ?3 AND turn = ?4
               AND kind = ?5 AND text <> '' ORDER BY id DESC LIMIT 1"
        ))?;
        let mut previews = Vec::with_capacity(turns.len());
        for t in turns {
            let key = params![t.launch_id, t.area_id, t.lane, t.turn];
            let kinds = counts
                .query_map(key, |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut pick = |kind: &str| -> StorageResult<Option<String>> {
                Ok(summary
                    .query_row(params![t.launch_id, t.area_id, t.lane, t.turn, kind], |r| {
                        r.get(0)
                    })
                    .optional()?)
            };
            let text = match pick("reply")? {
                Some(text) => text,
                None => pick("prompt")?.unwrap_or_default(),
            };
            previews.push(BrainTurnPreview {
                launch_id: t.launch_id,
                area_id: t.area_id.clone(),
                lane: t.lane.clone(),
                turn: t.turn,
                counts: kinds,
                summary: text,
            });
        }
        Ok(previews)
    }

    /// 上次标记已读之后新增的动作数。
    pub fn unseen_actions(&self) -> StorageResult<usize> {
        let connection = open_app_connection(&self.path)?;
        let seen = seen_at(&connection)?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM brain_actions WHERE at_ms > ?1",
            [seen],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// 把“已读”位置推到 `at_ms`；不会往回退。
    pub fn mark_actions_seen(&self, at_ms: i64) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        connection.execute(
            "INSERT INTO brain_activity_state (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = MAX(value, excluded.value)",
            params![SEEN_KEY, at_ms],
        )?;
        Ok(())
    }
}

fn seen_at(connection: &rusqlite::Connection) -> StorageResult<i64> {
    Ok(connection
        .query_row(
            "SELECT value FROM brain_activity_state WHERE key = ?1",
            [SEEN_KEY],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
