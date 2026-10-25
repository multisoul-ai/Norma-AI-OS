//! 幕僚活动的持久记录（app 库）：对话轮次、轮次正文、自主动作，保留最近 7 天。
//!
//! 轮次号每次启动从头计数，所以主键带上 `launch_id`。本模块只存字符串与数字，不依赖 `brain`。

use crate::{migrations::open_app_connection, StorageResult};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const BRAIN_ACTIVITY_RETENTION_MS: i64 = 7 * 24 * 3600 * 1000;
/// 正文与标题的总字节上限；超过就从最旧的整轮开始删。
pub const BRAIN_ACTIVITY_MAX_TEXT_BYTES: usize = 32 * 1024 * 1024;

/// 读写共用：写入时 `finished_at_ms` / `stop_reason` 为空表示“尚未结束”，已有值不会被空值覆盖。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainTurnUpsert {
    pub launch_id: i64,
    pub area_id: String,
    pub lane: String,
    pub turn: i64,
    pub started_at_ms: i64,
    pub trigger_kind: String,
    pub trigger_who: String,
    pub finished_at_ms: Option<i64>,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainItemRow {
    pub launch_id: i64,
    pub area_id: String,
    pub lane: String,
    pub turn: i64,
    pub at_ms: i64,
    pub kind: String,
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainActionRow {
    pub launch_id: i64,
    pub area_id: String,
    pub at_ms: i64,
    pub kind: String,
    pub agent: String,
    pub node_id: i64,
    pub summary: String,
    pub reason: String,
    pub ask_key: String,
    /// 产生这个动作的轮次；用户自己批准的派发没有。
    pub via_lane: Option<String>,
    pub via_turn: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BrainActivityBatch {
    pub turns: Vec<BrainTurnUpsert>,
    pub items: Vec<BrainItemRow>,
    pub actions: Vec<BrainActionRow>,
}

impl BrainActivityBatch {
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty() && self.items.is_empty() && self.actions.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PruneReport {
    pub turns: usize,
    pub actions: usize,
}

#[derive(Clone, Debug)]
pub struct BrainActivityStore {
    pub(crate) path: PathBuf,
}

impl BrainActivityStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 一个事务写完一批；空批次不碰数据库。
    pub fn save_batch(&self, batch: &BrainActivityBatch) -> StorageResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        for t in &batch.turns {
            tx.execute(
                "INSERT INTO brain_turns (launch_id, area_id, lane, turn, started_at_ms,
                    trigger_kind, trigger_who, finished_at_ms, stop_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (launch_id, area_id, lane, turn) DO UPDATE SET
                    finished_at_ms = COALESCE(excluded.finished_at_ms, finished_at_ms),
                    stop_reason = COALESCE(excluded.stop_reason, stop_reason)",
                params![
                    t.launch_id,
                    t.area_id,
                    t.lane,
                    t.turn,
                    t.started_at_ms,
                    t.trigger_kind,
                    t.trigger_who,
                    t.finished_at_ms,
                    t.stop_reason
                ],
            )?;
        }
        for i in &batch.items {
            tx.execute(
                "INSERT INTO brain_trace_items
                    (launch_id, area_id, lane, turn, at_ms, kind, title, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    i.launch_id,
                    i.area_id,
                    i.lane,
                    i.turn,
                    i.at_ms,
                    i.kind,
                    i.title,
                    i.text
                ],
            )?;
        }
        for a in &batch.actions {
            tx.execute(
                "INSERT INTO brain_actions (launch_id, area_id, at_ms, kind, agent, node_id,
                    summary, reason, ask_key, via_lane, via_turn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    a.launch_id,
                    a.area_id,
                    a.at_ms,
                    a.kind,
                    a.agent,
                    a.node_id,
                    a.summary,
                    a.reason,
                    a.ask_key,
                    a.via_lane,
                    a.via_turn
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 最近的轮次，新的在前。`before_ms` 是上一页最后一条的开始时间。
    // shortcut: 游标只按毫秒比较，同一毫秒开始的轮次跨页时会漏；轮次间隔以秒计，需要时改成复合游标。
    pub fn recent_turns(
        &self,
        limit: usize,
        before_ms: Option<i64>,
    ) -> StorageResult<Vec<BrainTurnUpsert>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT launch_id, area_id, lane, turn, started_at_ms, trigger_kind, trigger_who,
                    finished_at_ms, stop_reason
             FROM brain_turns WHERE started_at_ms < ?1
             ORDER BY started_at_ms DESC, launch_id DESC, turn DESC LIMIT ?2",
        )?;
        let rows =
            statement.query_map(params![before_ms.unwrap_or(i64::MAX), limit as i64], |r| {
                Ok(BrainTurnUpsert {
                    launch_id: r.get(0)?,
                    area_id: r.get(1)?,
                    lane: r.get(2)?,
                    turn: r.get(3)?,
                    started_at_ms: r.get(4)?,
                    trigger_kind: r.get(5)?,
                    trigger_who: r.get(6)?,
                    finished_at_ms: r.get(7)?,
                    stop_reason: r.get(8)?,
                })
            })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 某一轮的正文，按写入顺序。
    pub fn turn_items(
        &self,
        launch_id: i64,
        area_id: &str,
        lane: &str,
        turn: i64,
    ) -> StorageResult<Vec<BrainItemRow>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT launch_id, area_id, lane, turn, at_ms, kind, title, text
             FROM brain_trace_items
             WHERE launch_id = ?1 AND area_id = ?2 AND lane = ?3 AND turn = ?4 ORDER BY id",
        )?;
        let rows = statement.query_map(params![launch_id, area_id, lane, turn], |r| {
            Ok(BrainItemRow {
                launch_id: r.get(0)?,
                area_id: r.get(1)?,
                lane: r.get(2)?,
                turn: r.get(3)?,
                at_ms: r.get(4)?,
                kind: r.get(5)?,
                title: r.get(6)?,
                text: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn recent_actions(
        &self,
        limit: usize,
        before_ms: Option<i64>,
    ) -> StorageResult<Vec<BrainActionRow>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT launch_id, area_id, at_ms, kind, agent, node_id, summary, reason, ask_key,
                    via_lane, via_turn
             FROM brain_actions WHERE at_ms < ?1 ORDER BY at_ms DESC, id DESC LIMIT ?2",
        )?;
        let rows =
            statement.query_map(params![before_ms.unwrap_or(i64::MAX), limit as i64], |r| {
                Ok(BrainActionRow {
                    launch_id: r.get(0)?,
                    area_id: r.get(1)?,
                    at_ms: r.get(2)?,
                    kind: r.get(3)?,
                    agent: r.get(4)?,
                    node_id: r.get(5)?,
                    summary: r.get(6)?,
                    reason: r.get(7)?,
                    ask_key: r.get(8)?,
                    via_lane: r.get(9)?,
                    via_turn: r.get(10)?,
                })
            })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 正文与标题的总字节数。
    pub fn text_bytes(&self) -> StorageResult<usize> {
        text_bytes(&open_app_connection(&self.path)?)
    }

    /// 删掉 7 天前的内容，再把正文总量压回上限内。
    pub fn prune(&self, now_ms: i64) -> StorageResult<PruneReport> {
        let mut connection = open_app_connection(&self.path)?;
        let cutoff = now_ms - BRAIN_ACTIVITY_RETENTION_MS;
        let tx = connection.transaction()?;
        let mut report = PruneReport {
            turns: tx.execute("DELETE FROM brain_turns WHERE started_at_ms < ?1", [cutoff])?,
            actions: tx.execute("DELETE FROM brain_actions WHERE at_ms < ?1", [cutoff])?,
        };
        tx.execute("DELETE FROM brain_trace_items WHERE at_ms < ?1", [cutoff])?;
        delete_orphan_items(&tx)?;
        report.turns += enforce_byte_cap(&tx)?;
        tx.commit()?;
        Ok(report)
    }

    pub fn clear(&self) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        connection.execute_batch(
            "DELETE FROM brain_trace_items; DELETE FROM brain_turns; DELETE FROM brain_actions;",
        )?;
        Ok(())
    }
}

fn delete_orphan_items(connection: &Connection) -> StorageResult<()> {
    connection.execute(
        "DELETE FROM brain_trace_items WHERE NOT EXISTS (
            SELECT 1 FROM brain_turns t WHERE t.launch_id = brain_trace_items.launch_id
              AND t.area_id = brain_trace_items.area_id AND t.lane = brain_trace_items.lane
              AND t.turn = brain_trace_items.turn)",
        [],
    )?;
    Ok(())
}

/// 从最旧的整轮开始删，直到正文总量不超过上限；返回删掉的轮数。
fn enforce_byte_cap(connection: &Connection) -> StorageResult<usize> {
    let mut excess = text_bytes(connection)?.saturating_sub(BRAIN_ACTIVITY_MAX_TEXT_BYTES);
    if excess == 0 {
        return Ok(0);
    }
    let mut statement = connection.prepare(
        "SELECT t.rowid, COALESCE(SUM(LENGTH(CAST(i.text AS BLOB)) + LENGTH(CAST(i.title AS BLOB))), 0)
         FROM brain_turns t LEFT JOIN brain_trace_items i
           ON i.launch_id = t.launch_id AND i.area_id = t.area_id AND i.lane = t.lane AND i.turn = t.turn
         GROUP BY t.rowid ORDER BY t.started_at_ms ASC",
    )?;
    let mut doomed: Vec<i64> = Vec::new();
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize))
    })?;
    for row in rows {
        let (rowid, bytes) = row?;
        doomed.push(rowid);
        excess = excess.saturating_sub(bytes);
        if excess == 0 {
            break;
        }
    }
    drop(statement);
    for rowid in &doomed {
        connection.execute("DELETE FROM brain_turns WHERE rowid = ?1", [rowid])?;
    }
    delete_orphan_items(connection)?;
    Ok(doomed.len())
}

fn text_bytes(connection: &Connection) -> StorageResult<usize> {
    let total: i64 = connection.query_row(
        "SELECT COALESCE(SUM(LENGTH(CAST(text AS BLOB)) + LENGTH(CAST(title AS BLOB))), 0)
         FROM brain_trace_items",
        [],
        |r| r.get(0),
    )?;
    Ok(total as usize)
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS brain_turns (
            launch_id INTEGER NOT NULL,
            area_id TEXT NOT NULL,
            lane TEXT NOT NULL,
            turn INTEGER NOT NULL,
            started_at_ms INTEGER NOT NULL,
            trigger_kind TEXT NOT NULL,
            trigger_who TEXT NOT NULL,
            finished_at_ms INTEGER,
            stop_reason TEXT,
            PRIMARY KEY (launch_id, area_id, lane, turn)
        );
        CREATE INDEX IF NOT EXISTS idx_brain_turns_started ON brain_turns(started_at_ms);
        CREATE TABLE IF NOT EXISTS brain_trace_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            launch_id INTEGER NOT NULL,
            area_id TEXT NOT NULL,
            lane TEXT NOT NULL,
            turn INTEGER NOT NULL,
            at_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            text TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_brain_items_turn
            ON brain_trace_items(launch_id, area_id, lane, turn);
        CREATE INDEX IF NOT EXISTS idx_brain_items_at ON brain_trace_items(at_ms);
        CREATE TABLE IF NOT EXISTS brain_actions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            launch_id INTEGER NOT NULL,
            area_id TEXT NOT NULL,
            at_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            agent TEXT NOT NULL,
            node_id INTEGER NOT NULL,
            summary TEXT NOT NULL,
            reason TEXT NOT NULL,
            ask_key TEXT NOT NULL,
            via_lane TEXT,
            via_turn INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_brain_actions_at ON brain_actions(at_ms);
        CREATE TABLE IF NOT EXISTS brain_activity_state (
            key TEXT PRIMARY KEY NOT NULL,
            value INTEGER NOT NULL
        );
        "#,
    )?;
    Ok(())
}
