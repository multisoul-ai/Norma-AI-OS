//! 待办想法的手动排序：只改 `position`，不碰状态；位置取邻居中点，间隙耗尽时整体重排。

use crate::ideas_store::{IdeasStore, POSITION_STEP};
use crate::StorageResult;
use rusqlite::{params, TransactionBehavior};

impl IdeasStore {
    /// 把待办想法 `id` 移到 `before` 之前；`before` 为 `None` 表示移到待办末尾。
    /// 返回 `Ok(false)` 表示没有改动：想法不是待办、`before` 不是待办、两者相同，或已在目标位置。
    pub fn move_before(
        &self,
        workspace_id: &str,
        id: &str,
        before: Option<&str>,
    ) -> StorageResult<bool> {
        if before == Some(id) {
            return Ok(false);
        }
        let mut connection = self.open()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let todo: Vec<(String, i64)> = {
            let mut query = tx.prepare(
                "SELECT id,position FROM workspace_ideas WHERE workspace_id=?1 AND state='todo' ORDER BY position,id",
            )?;
            let rows = query.query_map([workspace_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let known = |wanted: &str| todo.iter().any(|(row_id, _)| row_id == wanted);
        if !known(id) || before.is_some_and(|wanted| !known(wanted)) {
            return Ok(false);
        }
        let mut rest: Vec<&(String, i64)> =
            todo.iter().filter(|(row_id, _)| row_id != id).collect();
        let slot = match before {
            Some(wanted) => rest.iter().position(|(row_id, _)| row_id == wanted),
            None => Some(rest.len()),
        }
        .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let current = todo.iter().position(|(row_id, _)| row_id == id);
        if current == Some(slot) {
            return Ok(false);
        }
        let prev = slot.checked_sub(1).map(|index| rest[index].1);
        let next = rest.get(slot).map(|row| row.1);
        let target = match (prev, next) {
            (None, None) => POSITION_STEP,
            (None, Some(next)) => next - POSITION_STEP,
            (Some(prev), None) => prev + POSITION_STEP,
            (Some(prev), Some(next)) if next - prev >= 2 => prev + (next - prev) / 2,
            _ => {
                // 间隙耗尽：按新顺序整体重排。
                let moving = todo.iter().find(|(row_id, _)| row_id == id);
                rest.splice(slot..slot, moving);
                for (index, (row_id, _)) in rest.iter().enumerate() {
                    tx.execute(
                        "UPDATE workspace_ideas SET position=?3 WHERE workspace_id=?1 AND id=?2",
                        params![workspace_id, row_id, (index as i64 + 1) * POSITION_STEP],
                    )?;
                }
                tx.commit()?;
                return Ok(true);
            }
        };
        tx.execute(
            "UPDATE workspace_ideas SET position=?3 WHERE workspace_id=?1 AND id=?2",
            params![workspace_id, id, target],
        )?;
        tx.commit()?;
        Ok(true)
    }
}
