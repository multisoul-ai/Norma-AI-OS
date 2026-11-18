//! 幕僚任务唯一写入口。只在后台调用；成功落盘后才发布新快照。
use norma::protocol::Task;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
static STORES: OnceLock<Mutex<BTreeMap<PathBuf, Weak<TaskStore>>>> = OnceLock::new();
mod assign;
mod delivery;
pub mod gate;
pub mod naming;
pub mod policy;
mod proposals;
mod questions;
pub mod worker_life;
mod workflow;

pub use assign::AssignOutcome;

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

/// v3：Worker 只保留 `phase`。v1 备份为 `tasks.v1.bak.json`，v2 备份为 `tasks.v2.bak.json`，都不迁移。
const SNAPSHOT_VERSION: u32 = 3;

#[derive(Deserialize)]
struct SnapshotHeader {
    version: u32,
}

pub struct TaskStore {
    root: PathBuf,
    snapshots: Mutex<BTreeMap<String, Snapshot>>,
    /// 本次加载把未结束 Worker 收成重启失败的任务。宿主取走后唤醒幕僚一次。
    restart_wakes: Mutex<Vec<(String, String)>>,
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
        if let BrainEventKind::WorkerWithdrawn {
            node_id,
            dispatch_id,
        } = event
        {
            return self.record_withdraw(workspace, *node_id, dispatch_id);
        }
        if let BrainEventKind::WorkerDelivery {
            node_id,
            dispatch_id,
            phase,
        } = event
        {
            return self.record_delivery(workspace, *node_id, dispatch_id, *phase);
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
        let task_id = match locate_worker(&tasks, *node_id, dispatch_id.as_deref()) {
            WorkerLocate::Found(id) => id,
            WorkerLocate::Unowned => return Ok(TaskEventResult::Unowned),
            WorkerLocate::Ignored => return Ok(TaskEventResult::Ignored),
        };
        let Some(task) = tasks.iter().find(|task| task.id == task_id) else {
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
                // 播报与否由 triage 决定；幕僚要看到每个被接受的结果，包括普通成功和中断。
                wake: true,
            }
        } else {
            TaskEventResult::Ignored
        })
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            snapshots: Mutex::default(),
            restart_wakes: Mutex::default(),
        }
    }

    /// 取走加载时产生的重启唤醒。每个任务只出现一次，不增加 `auto_dispatches`。
    pub fn take_restart_wakes(&self) -> Vec<(String, String)> {
        self.restart_wakes
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn path(&self, workspace: &str) -> PathBuf {
        self.root
            .join("projects")
            .join(crate::migration::workspace_hash(workspace))
            .join("tasks.json")
    }

    fn load(&self, workspace: &str) -> io::Result<Snapshot> {
        let path = self.path(workspace);
        let fresh = || Snapshot {
            version: SNAPSHOT_VERSION,
            workspace_id: workspace.into(),
            tasks: Vec::new(),
        };
        let mut snapshot: Snapshot = match std::fs::read(&path) {
            // 先只读版本号：旧版的消息格式已不存在，整体反序列化必然失败，不能当成损坏文件。
            Ok(bytes) => match serde_json::from_slice::<SnapshotHeader>(&bytes)?.version {
                version if version < 2 => {
                    std::fs::rename(&path, path.with_file_name("tasks.v1.bak.json"))?;
                    fresh()
                }
                version if version < SNAPSHOT_VERSION => {
                    std::fs::rename(&path, path.with_file_name("tasks.v2.bak.json"))?;
                    fresh()
                }
                _ => serde_json::from_slice(&bytes)?,
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => fresh(),
            Err(error) => return Err(error),
        };
        let ids: BTreeSet<_> = snapshot.tasks.iter().map(|task| &task.id).collect();
        if snapshot.version != SNAPSHOT_VERSION
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
        let mut expired = false;
        let mut wakes = Vec::new();
        for task in &mut snapshot.tasks {
            // 崩溃遗留：仍是 Running 的幕僚回复记为 Failed，与过期的待答问题一起修复一次。
            // 没结束的 Worker 进程已经不在了，记为重启失败；这一次只唤醒，不加自动派发次数。
            let restarted = worker_life::fail_restarted(task);
            let recovered = task.recover_interrupted();
            if task.expire_pending_questions() || recovered || restarted {
                task.revision = task
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("task revision overflow"))?;
                task.updated_at = norma::protocol::timestamp();
                expired = true;
            }
            if restarted {
                wakes.push(task.id.clone());
            }
        }
        if expired {
            crate::persistence::atomic_write(&path, &serde_json::to_vec(&snapshot)?)?;
        }
        if !wakes.is_empty() {
            if let Ok(mut queue) = self.restart_wakes.lock() {
                for id in wakes {
                    queue.push((workspace.to_owned(), id));
                }
            }
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

    /// 幕僚每轮开始：轮次加一并追加一条 Running 的幕僚回复，同一次事务落盘。
    pub fn begin_turn(
        &self,
        workspace: &str,
        id: &str,
        trigger: norma::protocol::ReplyTrigger,
    ) -> io::Result<Task> {
        self.update(workspace, id, |task| {
            // 新一轮开始时仍在运行的回复一定是上一个服务实例遗留的，记为失败，不留永远转圈的回复。
            task.recover_interrupted();
            task.user_words = naming::user_words(&task.messages);
            task.round += 1;
            task.begin_reply(trigger);
            Ok(())
        })
    }

    /// 结束本轮：仍在运行的回复记为 `status` 并落盘；本轮已经没有运行中的回复且没有要写的会话号时什么也不写。
    pub fn settle_reply(
        &self,
        workspace: &str,
        id: &str,
        status: norma::protocol::ReplyStatus,
        elapsed_ms: u64,
        session_id: Option<&str>,
    ) -> io::Result<Option<Task>> {
        let running = self.update_live(workspace, id, |task| task.running_reply_mut().is_some())?;
        if !running && session_id.is_none() {
            return Ok(None);
        }
        self.update(workspace, id, |task| {
            if let Some(session_id) = session_id {
                task.acp_session_id = Some(session_id.into());
            }
            if let Some(reply) = task.running_reply_mut() {
                reply.finish(status, elapsed_ms);
            }
            // 只有正常结束的轮次才整批替换；中断或出错时保留旧方案。
            if status == norma::protocol::ReplyStatus::Done {
                task.supersede_stale_proposals();
            }
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })
        .map(Some)
    }

    /// 撤掉还没写出任何内容的运行中回复（临时故障重试前用，避免留下空的失败回复）。
    pub fn discard_empty_reply(&self, workspace: &str, id: &str) -> io::Result<Option<Task>> {
        let empty = self.update_live(workspace, id, |task| {
            task.running_reply_mut()
                .is_some_and(|reply| reply.parts.is_empty())
        })?;
        if !empty {
            return Ok(None);
        }
        self.update(workspace, id, |task| {
            if let Some(index) = task.messages.iter().rposition(|message| {
                matches!(message, norma::protocol::TaskMessage::Staff(reply) if reply.is_running())
            }) {
                task.messages.remove(index);
            }
            Ok(())
        })
        .map(Some)
    }

    /// 流式期间的实时更新：只改内存快照，不写盘、不加 revision，也不克隆整份快照。
    /// 调用方自己决定返回什么（通常只是本轮回复），广播不必带上整个任务。
    pub fn update_live<T>(
        &self,
        workspace: &str,
        id: &str,
        change: impl FnOnce(&mut Task) -> T,
    ) -> io::Result<T> {
        let mut snapshots = self
            .snapshots
            .lock()
            .map_err(|_| io::Error::other("staff store lock"))?;
        if !snapshots.contains_key(workspace) {
            let loaded = self.load(workspace)?;
            snapshots.insert(workspace.into(), loaded);
        }
        let task = snapshots
            .get_mut(workspace)
            .and_then(|snapshot| snapshot.tasks.iter_mut().find(|task| task.id == id))
            .ok_or_else(missing)?;
        Ok(change(task))
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

pub(super) enum WorkerLocate {
    Unowned,
    Ignored,
    Found(String),
}

/// 卡片和派发编号一起才能定位。缺派发编号是这条卡上的过期事件；对不上任何一对则不能落到别的任务。
pub(super) fn locate_worker(tasks: &[Task], node: u64, dispatch: Option<&str>) -> WorkerLocate {
    let has_card = tasks.iter().any(|task| {
        task.workers
            .iter()
            .any(|worker| worker.card_id == Some(node))
    });
    if !has_card {
        return WorkerLocate::Unowned;
    }
    let Some(dispatch) = dispatch.filter(|dispatch| !dispatch.is_empty()) else {
        return WorkerLocate::Ignored;
    };
    tasks
        .iter()
        .find(|task| {
            task.workers
                .iter()
                .any(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
        })
        .map(|task| WorkerLocate::Found(task.id.clone()))
        .unwrap_or(WorkerLocate::Unowned)
}
