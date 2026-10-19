use super::*;

#[derive(Clone, Debug)]
pub struct LiveTaskWatch {
    pub id: i64,
    pub project_id: String,
    pub task_ref: String,
}
#[derive(Clone, Debug)]
pub struct LiveTaskFact {
    pub event_id: String,
    pub content: String,
}
impl LiveOperationStore {
    /// Cursor pages rotate across running tasks so an old long task cannot starve later completions.
    pub fn task_watches(&self, project: &str, after: i64) -> StorageResult<Vec<LiveTaskWatch>> {
        validate_project(project)?;
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare("SELECT id,task_ref FROM live_task_watches WHERE project_id=?1 AND finished=0 AND id>?2 ORDER BY id LIMIT 100")?;
        let rows = query.query_map(params![project, after], |row| {
            Ok(LiveTaskWatch {
                id: row.get(0)?,
                project_id: project.into(),
                task_ref: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn queue_task_fact(
        &self,
        watch: &LiveTaskWatch,
        state: &str,
        content: &str,
    ) -> StorageResult<()> {
        if !matches!(state, "succeeded" | "failed" | "cancelled" | "interrupted")
            || content.is_empty()
            || content.len() > 480
        {
            return Err(invalid("invalid terminal task fact"));
        }
        let connection = open_app_connection(&self.path)?;
        let identity = serde_json::to_string(&(&watch.project_id, &watch.task_ref, state))?;
        let event_id = format!("live-task-{:x}", Sha256::digest(identity.as_bytes()));
        let changed = connection.execute("INSERT OR IGNORE INTO live_task_facts(event_id,watch_id,project_id,terminal_state,content) SELECT ?1,id,project_id,?2,?3 FROM live_task_watches WHERE id=?4 AND project_id=?5 AND task_ref=?6",params![event_id,state,content,watch.id,watch.project_id,watch.task_ref])?;
        if changed == 0 {
            let previous: Option<String> = connection
                .query_row(
                    "SELECT content FROM live_task_facts WHERE event_id=?1",
                    [event_id],
                    |row| row.get(0),
                )
                .optional()?;
            if previous.as_deref() != Some(content) {
                return Err(invalid(
                    "task fact identity missing or immutable content conflicts",
                ));
            }
        }
        Ok(())
    }

    pub fn pending_task_facts(&self, project: &str) -> StorageResult<Vec<LiveTaskFact>> {
        validate_project(project)?;
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare("SELECT event_id,content FROM live_task_facts WHERE project_id=?1 AND append_ack=0 ORDER BY rowid LIMIT 100")?;
        let rows = query.query_map([project], |row| {
            Ok(LiveTaskFact {
                event_id: row.get(0)?,
                content: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// ACK means the service accepted appended facts; it never proves complete audible playback.
    pub fn acknowledge_task_fact(&self, project: &str, event_id: &str) -> StorageResult<bool> {
        validate_project(project)?;
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let watch: Option<i64> = transaction
            .query_row(
                "SELECT watch_id FROM live_task_facts WHERE project_id=?1 AND event_id=?2",
                params![project, event_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = watch {
            transaction.execute(
                "UPDATE live_task_facts SET append_ack=1 WHERE project_id=?1 AND event_id=?2",
                params![project, event_id],
            )?;
            transaction.execute(
                "UPDATE live_task_watches SET finished=1 WHERE project_id=?1 AND id=?2",
                params![project, id],
            )?;
        }
        transaction.commit()?;
        Ok(watch.is_some())
    }
}

pub(super) fn register(
    connection: &Connection,
    key: &LiveOperationKey,
    outcome: &Outcome,
) -> StorageResult<()> {
    let Ok(result) = outcome else {
        return Ok(());
    };
    if result.status != "accepted" {
        return Ok(());
    }
    for task in &result.tasks {
        let Some(task_ref) = task.get("task_ref").and_then(Value::as_str) else {
            continue;
        };
        validate_id(task_ref)?;
        if task.get("task_id").and_then(Value::as_u64) != result.task_id
            || task.get("node_id").and_then(Value::as_u64) != result.node_id
            || task_ref.rsplit_once(':').and_then(|(runtime, id)| {
                (!runtime.is_empty())
                    .then(|| id.parse::<u64>().ok())
                    .flatten()
            }) != result.task_id
        {
            return Err(invalid(
                "stable task reference does not match the actual receipt",
            ));
        }
        connection.execute(
            "INSERT OR IGNORE INTO live_task_watches(project_id,task_ref) VALUES(?1,?2)",
            params![key.project_id, task_ref],
        )?;
    }
    Ok(())
}

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS live_task_watches(id INTEGER PRIMARY KEY,project_id TEXT NOT NULL,task_ref TEXT NOT NULL,finished INTEGER NOT NULL DEFAULT 0,UNIQUE(project_id,task_ref));
    CREATE TABLE IF NOT EXISTS live_task_facts(event_id TEXT PRIMARY KEY,watch_id INTEGER NOT NULL,project_id TEXT NOT NULL,terminal_state TEXT NOT NULL,content TEXT NOT NULL,append_ack INTEGER NOT NULL DEFAULT 0,UNIQUE(watch_id,terminal_state));")?;
    Ok(())
}
