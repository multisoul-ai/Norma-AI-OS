use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use std::path::PathBuf;
use workspace::ideas::{normalize_body, transition, IdeaEvent, IdeaSource, IdeaState};

/// 相邻想法的排序间隔，为将来的手动排序留出空隙。
const POSITION_STEP: i64 = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredIdea {
    pub id: String,
    pub workspace_id: String,
    pub body: String,
    pub source: IdeaSource,
    pub state: IdeaState,
    pub task_ref: Option<String>,
    pub assignee_area_id: Option<String>,
    pub assignee_node_id: Option<u64>,
    pub position: i64,
    pub created_at_unix: u64,
}

/// 工作空间想法清单。状态流转只走 `workspace::ideas::transition`；
/// 返回 `Ok(None)` 表示想法不存在或当前状态不允许该事件（例如并发中输掉的一方）。
#[derive(Clone, Debug)]
pub struct IdeasStore {
    path: PathBuf,
}

enum Patch<'a> {
    Keep,
    TaskRef(&'a str),
    Assignee {
        area: &'a str,
        node: u64,
        task_ref: Option<&'a str>,
    },
    Clear,
}

impl IdeasStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 由存储生成 ID 的 `add`，供对话/语音入口使用。
    pub fn capture(
        &self,
        workspace_id: &str,
        body: &str,
        source: IdeaSource,
    ) -> StorageResult<StoredIdea> {
        let id = crate::workspace_identity_store::random_workspace_id()?;
        self.add(workspace_id, &id, body, source)
    }

    pub fn add(
        &self,
        workspace_id: &str,
        id: &str,
        body: &str,
        source: IdeaSource,
    ) -> StorageResult<StoredIdea> {
        crate::mobile_access_store::validate_id(id)?;
        crate::mobile_access_store::validate_id(workspace_id)?;
        let body = normalize_body(body).map_err(|error| invalid(&format!("{error:?}")))?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position),0)+?2 FROM workspace_ideas WHERE workspace_id=?1",
            params![workspace_id, POSITION_STEP],
            |row| row.get(0),
        )?;
        let now = crate::current_unix_timestamp()? as i64;
        tx.execute(
            "INSERT INTO workspace_ideas(id,workspace_id,body,source,state,position,created_at_unix,updated_at_unix)
             VALUES(?1,?2,?3,?4,'todo',?5,?6,?6)",
            params![id, workspace_id, body, source.as_str(), position, now],
        )?;
        let idea = read(&tx, workspace_id, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        tx.commit()?;
        Ok(idea)
    }

    /// 清单按记录顺序，不含已删除；已完成的保留，由界面折叠。
    pub fn list(&self, workspace_id: &str) -> StorageResult<Vec<StoredIdea>> {
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(&format!(
            "{SELECT} WHERE workspace_id=?1 AND state<>'dismissed' ORDER BY position,id"
        ))?;
        let rows = query.query_map([workspace_id], decode)?;
        rows.map(|row| convert(row?)).collect()
    }

    pub fn begin_dispatch(
        &self,
        workspace_id: &str,
        id: &str,
        task_ref: &str,
    ) -> StorageResult<Option<StoredIdea>> {
        validate_task_ref(task_ref)?;
        self.step(
            workspace_id,
            id,
            IdeaEvent::BeginDispatch,
            Patch::TaskRef(task_ref),
        )
    }

    /// 派发成功：记录落点智能体，并用派发回执里的真实 `task_ref` 替换占位值（未给出则保留）。
    pub fn mark_assigned(
        &self,
        workspace_id: &str,
        id: &str,
        area_id: &str,
        node_id: u64,
        task_ref: Option<&str>,
    ) -> StorageResult<Option<StoredIdea>> {
        if let Some(task_ref) = task_ref {
            validate_task_ref(task_ref)?;
        }
        let patch = Patch::Assignee {
            area: area_id,
            node: node_id,
            task_ref,
        };
        self.step(workspace_id, id, IdeaEvent::DispatchSucceeded, patch)
    }

    pub fn fail_dispatch(&self, workspace_id: &str, id: &str) -> StorageResult<Option<StoredIdea>> {
        self.step(workspace_id, id, IdeaEvent::DispatchFailed, Patch::Clear)
    }

    pub fn retract(&self, workspace_id: &str, id: &str) -> StorageResult<Option<StoredIdea>> {
        self.step(workspace_id, id, IdeaEvent::Retract, Patch::Clear)
    }

    pub fn dismiss(&self, workspace_id: &str, id: &str) -> StorageResult<Option<StoredIdea>> {
        self.step(workspace_id, id, IdeaEvent::Dismiss, Patch::Keep)
    }

    /// 删除保留原排序值，撤销即回到原位。
    pub fn restore(&self, workspace_id: &str, id: &str) -> StorageResult<Option<StoredIdea>> {
        self.step(workspace_id, id, IdeaEvent::Restore, Patch::Keep)
    }

    /// 任务终态回写：只认 `task_ref`，不按文本猜测。返回是否有想法被置为已完成。
    pub fn complete_by_task_ref(&self, task_ref: &str) -> StorageResult<bool> {
        let connection = open_app_connection(&self.path)?;
        let now = crate::current_unix_timestamp()? as i64;
        let changed = connection.execute(
            "UPDATE workspace_ideas SET state='done',updated_at_unix=?2 WHERE task_ref=?1 AND state='assigned'",
            params![task_ref, now],
        )?;
        Ok(changed > 0)
    }

    /// 启动对账：遗留的“派发中”按 `task_ref` 是否真的发出来决定去向，
    /// 发出的补成已指派，没发出的回待办；不盲目回退，避免重复派发。返回被处理的条数。
    pub fn reconcile_dispatching(
        &self,
        workspace_id: &str,
        was_dispatched: impl Fn(&str) -> bool,
    ) -> StorageResult<usize> {
        let stuck: Vec<StoredIdea> = self
            .list(workspace_id)?
            .into_iter()
            .filter(|idea| idea.state == IdeaState::Dispatching)
            .collect();
        let mut settled = 0;
        for idea in stuck {
            let sent = idea.task_ref.as_deref().is_some_and(&was_dispatched);
            let result = if sent {
                self.step(
                    workspace_id,
                    &idea.id,
                    IdeaEvent::DispatchSucceeded,
                    Patch::Keep,
                )?
            } else {
                self.step(
                    workspace_id,
                    &idea.id,
                    IdeaEvent::DispatchFailed,
                    Patch::Clear,
                )?
            };
            settled += usize::from(result.is_some());
        }
        Ok(settled)
    }

    /// 在立即事务里读状态、校验流转、条件更新：并发时只有一个调用方能成功。
    fn step(
        &self,
        workspace_id: &str,
        id: &str,
        event: IdeaEvent,
        patch: Patch<'_>,
    ) -> StorageResult<Option<StoredIdea>> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(current) = read(&tx, workspace_id, id)? else {
            return Ok(None);
        };
        let Ok(next) = transition(current.state, event) else {
            return Ok(None);
        };
        let now = crate::current_unix_timestamp()? as i64;
        let head = "UPDATE workspace_ideas SET state=?1,updated_at_unix=?2";
        let tail = "WHERE workspace_id=?3 AND id=?4 AND state=?5";
        let base = params![next.as_str(), now, workspace_id, id, current.state.as_str()];
        let changed = match patch {
            Patch::Keep => tx.execute(&format!("{head} {tail}"), base)?,
            Patch::Clear => tx.execute(
                &format!("{head},task_ref=NULL,assignee_area_id=NULL,assignee_node_id=NULL {tail}"),
                base,
            )?,
            Patch::TaskRef(task_ref) => tx.execute(
                &format!("{head},task_ref=?6 {tail}"),
                params![
                    next.as_str(),
                    now,
                    workspace_id,
                    id,
                    current.state.as_str(),
                    task_ref
                ],
            )?,
            Patch::Assignee {
                area,
                node,
                task_ref,
            } => tx.execute(
                &format!(
                    "{head},assignee_area_id=?6,assignee_node_id=?7,task_ref=COALESCE(?8,task_ref) {tail}"
                ),
                params![
                    next.as_str(),
                    now,
                    workspace_id,
                    id,
                    current.state.as_str(),
                    area,
                    i64::try_from(node).map_err(|_| invalid("node id out of range"))?,
                    task_ref
                ],
            )?,
        };
        if changed == 0 {
            return Ok(None);
        }
        let idea = read(&tx, workspace_id, id)?;
        tx.commit()?;
        Ok(idea)
    }
}

const SELECT: &str = "SELECT id,workspace_id,body,source,state,task_ref,assignee_area_id,assignee_node_id,position,created_at_unix FROM workspace_ideas";

type Raw = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
);

fn decode(row: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn convert(raw: Raw) -> StorageResult<StoredIdea> {
    let (id, workspace_id, body, source, state, task_ref, area, node, position, created) = raw;
    Ok(StoredIdea {
        source: IdeaSource::parse(&source).ok_or_else(|| invalid("unknown source"))?,
        state: IdeaState::parse(&state).ok_or_else(|| invalid("unknown state"))?,
        id,
        workspace_id,
        body,
        task_ref,
        assignee_area_id: area,
        assignee_node_id: node.map(|value| value as u64),
        position,
        created_at_unix: created as u64,
    })
}

fn read(tx: &Transaction<'_>, workspace_id: &str, id: &str) -> StorageResult<Option<StoredIdea>> {
    tx.query_row(
        &format!("{SELECT} WHERE workspace_id=?1 AND id=?2"),
        params![workspace_id, id],
        decode,
    )
    .optional()?
    .map(convert)
    .transpose()
}

/// 运行时 task_ref 形如 `<runtime-instance>:<id>`，不能套用只允许字母数字的 ID 规则。
fn validate_task_ref(task_ref: &str) -> StorageResult<()> {
    if task_ref.is_empty() || task_ref.len() > 128 || task_ref.chars().any(char::is_control) {
        return Err(invalid("invalid task_ref"));
    }
    Ok(())
}

fn invalid(message: &str) -> StorageError {
    StorageError::InvalidIdea(message.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_ideas (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            source TEXT NOT NULL,
            state TEXT NOT NULL,
            task_ref TEXT,
            assignee_area_id TEXT,
            assignee_node_id INTEGER,
            position INTEGER NOT NULL,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_ideas_workspace
        ON workspace_ideas(workspace_id, state, position);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_ideas_task_ref
        ON workspace_ideas(task_ref) WHERE task_ref IS NOT NULL;",
    )?;
    Ok(())
}
