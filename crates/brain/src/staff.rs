//! 幕僚任务唯一写入口。只在后台调用；成功落盘后才发布新快照。
use norma::protocol::Task;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
static STORES: OnceLock<Mutex<BTreeMap<PathBuf, Weak<TaskStore>>>> = OnceLock::new();
mod workflow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskEventResult {
    Unowned,
    Ignored,
    Recorded { task_id: String, wake: bool },
}

#[derive(Clone, Deserialize, Serialize)]
struct Snapshot {
    version: u32,
    workspace_id: String,
    tasks: Vec<Task>,
}

pub struct TaskStore {
    root: PathBuf,
    snapshots: Mutex<BTreeMap<String, Snapshot>>,
}

impl TaskStore {
    /// App generations retain one writer for a state directory while any old operation is alive.
    pub fn shared(root: PathBuf) -> Arc<Self> {
        let mut stores = STORES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        stores.retain(|_, store| store.strong_count() > 0);
        if let Some(store) = stores.get(&root).and_then(Weak::upgrade) {
            return store;
        }
        let store = Arc::new(Self::new(root.clone()));
        stores.insert(root, Arc::downgrade(&store));
        store
    }

    pub fn record_event(
        &self,
        workspace: &str,
        event: &crate::BrainEventKind,
    ) -> io::Result<TaskEventResult> {
        use crate::{BrainEventKind, Outcome};
        use norma::protocol::WorkerOutcome;
        if let BrainEventKind::WorkerAttention {
            node_id,
            dispatch_id,
            waiting,
        } = event
        {
            return self.record_attention(workspace, *node_id, dispatch_id, *waiting);
        }
        let BrainEventKind::AgentFinished {
            node_id,
            dispatch_id,
            outcome,
            excerpt,
            from_result,
            ..
        } = event
        else {
            return Ok(TaskEventResult::Unowned);
        };
        let tasks = self.list(workspace)?;
        let Some(task) = tasks.iter().find(|task| {
            task.workers
                .iter()
                .any(|worker| worker.card_id == Some(*node_id))
        }) else {
            return Ok(TaskEventResult::Unowned);
        };
        let Some(dispatch_id) = dispatch_id.as_deref() else {
            return Ok(TaskEventResult::Ignored);
        };
        let outcome = match outcome {
            Outcome::Succeeded if *from_result => WorkerOutcome::Succeeded,
            Outcome::Succeeded => WorkerOutcome::Unavailable,
            Outcome::Failed => WorkerOutcome::Failed,
            Outcome::Interrupted => WorkerOutcome::Interrupted,
        };
        let mut accepted = false;
        self.update(workspace, &task.id, |task| {
            accepted = task.finish_worker(*node_id, dispatch_id, outcome, excerpt);
            Ok(())
        })?;
        Ok(if accepted {
            TaskEventResult::Recorded {
                task_id: task.id.clone(),
                wake: matches!(
                    crate::triage::triage(event),
                    crate::triage::Verdict::Speak(_)
                ),
            }
        } else {
            TaskEventResult::Ignored
        })
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            snapshots: Mutex::default(),
        }
    }

    pub fn path(&self, workspace: &str) -> PathBuf {
        self.root
            .join("projects")
            .join(crate::migration::workspace_hash(workspace))
            .join("tasks.json")
    }

    fn load(&self, workspace: &str) -> io::Result<Snapshot> {
        let path = self.path(workspace);
        let snapshot: Snapshot = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Snapshot {
                version: 1,
                workspace_id: workspace.into(),
                tasks: Vec::new(),
            },
            Err(error) => return Err(error),
        };
        let ids: BTreeSet<_> = snapshot.tasks.iter().map(|task| &task.id).collect();
        if snapshot.version != 1
            || snapshot.workspace_id != workspace
            || ids.len() != snapshot.tasks.len()
            || snapshot
                .tasks
                .iter()
                .any(|task| task.workspace_id != workspace || task.id.is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid staff snapshot identity",
            ));
        }
        Ok(snapshot)
    }

    fn transact<T>(
        &self,
        workspace: &str,
        save: bool,
        change: impl FnOnce(&mut Vec<Task>) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut snapshots = self
            .snapshots
            .lock()
            .map_err(|_| io::Error::other("staff store lock"))?;
        let mut next = match snapshots.get(workspace) {
            Some(snapshot) => snapshot.clone(),
            None => self.load(workspace)?,
        };
        let value = change(&mut next.tasks)?;
        if save {
            crate::persistence::atomic_write(&self.path(workspace), &serde_json::to_vec(&next)?)?;
        }
        snapshots.insert(workspace.into(), next);
        Ok(value)
    }

    pub fn list(&self, workspace: &str) -> io::Result<Vec<Task>> {
        self.transact(workspace, false, |tasks| Ok(tasks.clone()))
    }

    pub fn get(&self, workspace: &str, id: &str) -> io::Result<Task> {
        self.transact(workspace, false, |tasks| {
            tasks
                .iter()
                .find(|task| task.id == id)
                .cloned()
                .ok_or_else(missing)
        })
    }

    pub fn create(&self, task: Task) -> io::Result<Task> {
        let workspace = task.workspace_id.clone();
        self.transact(&workspace, true, |tasks| {
            if task.id.trim().is_empty()
                || workspace.trim().is_empty()
                || task.title.trim().is_empty()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "empty task identity or title",
                ));
            }
            if tasks.iter().any(|old| old.id == task.id) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "task already exists",
                ));
            }
            tasks.push(task.clone());
            Ok(task)
        })
    }

    pub fn update(
        &self,
        workspace: &str,
        id: &str,
        change: impl FnOnce(&mut Task) -> io::Result<()>,
    ) -> io::Result<Task> {
        self.transact(workspace, true, |tasks| {
            let task = tasks
                .iter_mut()
                .find(|task| task.id == id)
                .ok_or_else(missing)?;
            change(task)?;
            if task.id != id || task.workspace_id != workspace {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "task identity is immutable",
                ));
            }
            task.revision = task
                .revision
                .checked_add(1)
                .ok_or_else(|| io::Error::other("task revision overflow"))?;
            Ok(task.clone())
        })
    }
}

fn missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "staff task not found")
}
