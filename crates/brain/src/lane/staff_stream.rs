//! 把一轮的 ACP 流式事件写进当前这一条幕僚回复。
//! 文字和步骤开始只改内存并节流广播；步骤结束与回合结束才落盘。
use crate::acp::trace::{TraceItem, TraceKind};
use crate::event::{BrainStatus, BrainStatusKind};
use crate::staff::TaskStore;
use norma::protocol::{
    step_title, BrainScope, PlanItem, StaffReply, StepStatus, Task, TaskMessage,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    time::{Duration, Instant},
};

const LIVE_INTERVAL: Duration = Duration::from_millis(80);

pub(super) struct ReplyWriter {
    store: Arc<TaskStore>,
    scope: BrainScope,
    task_id: String,
    status: Sender<BrainStatus>,
    steps: HashMap<String, Instant>,
    last_sent: Option<Instant>,
    /// 被节流压住、还没广播的改动；窗口结束时由一次性线程补发，停顿前的最后几个字不会卡住。
    dirty: Arc<AtomicBool>,
    flush_scheduled: Arc<AtomicBool>,
}

/// 一次观察要对回复做的改动，以及它是否必须立刻广播、立刻落盘。
struct Change {
    apply: Box<dyn FnOnce(&mut StaffReply)>,
    forced: bool,
    durable: bool,
}

impl ReplyWriter {
    pub(super) fn new(
        store: Arc<TaskStore>,
        scope: BrainScope,
        task_id: String,
        status: Sender<BrainStatus>,
    ) -> Self {
        Self {
            store,
            scope,
            task_id,
            status,
            steps: HashMap::new(),
            last_sent: None,
            dirty: Arc::default(),
            flush_scheduled: Arc::default(),
        }
    }

    pub(super) fn observe(&mut self, item: &TraceItem) {
        let Some(change) = self.change(item) else {
            return;
        };
        if change.durable {
            self.save(change.apply);
        } else {
            self.live(change.apply, change.forced);
        }
    }

    fn change(&mut self, item: &TraceItem) -> Option<Change> {
        let live = |apply: Box<dyn FnOnce(&mut StaffReply)>, forced| Change {
            apply,
            forced,
            durable: false,
        };
        match item.kind {
            TraceKind::Reply => {
                let text = item.text.clone();
                Some(live(Box::new(move |reply| reply.push_text(&text)), false))
            }
            TraceKind::Prompt => Some(live(Box::new(StaffReply::clear_text), true)),
            TraceKind::Plan => {
                let items: Vec<PlanItem> = item
                    .text
                    .lines()
                    .filter_map(|line| {
                        let (status, text) = line.strip_prefix('[')?.split_once("] ")?;
                        Some(PlanItem {
                            status: status.into(),
                            text: text.into(),
                        })
                    })
                    .collect();
                Some(live(Box::new(move |reply| reply.set_plan(items)), false))
            }
            TraceKind::ToolCall | TraceKind::ToolResult | TraceKind::Permission => {
                let meta = item.tool.as_deref()?;
                let first_seen = !self.steps.contains_key(&meta.id);
                let started = *self
                    .steps
                    .entry(meta.id.clone())
                    .or_insert_with(Instant::now);
                let status = match meta.status.as_deref() {
                    Some("denied") => Some(StepStatus::Denied),
                    Some("completed") => Some(StepStatus::Completed),
                    Some("failed") => Some(StepStatus::Failed),
                    _ => None,
                };
                let finished = status.is_some();
                // 权限请求的名称可能是占位值：已知步骤的标题不被它覆盖。
                let title = (item.kind != TraceKind::Permission || first_seen)
                    .then(|| step_title(meta.name.as_deref(), meta.input.as_deref()));
                let elapsed = finished.then(|| started.elapsed().as_millis() as u64);
                let id = meta.id.clone();
                Some(Change {
                    apply: Box::new(move |reply| reply.upsert_step(&id, title, status, elapsed)),
                    forced: first_seen,
                    durable: finished,
                })
            }
            _ => None,
        }
    }

    /// 只改内存；`forced` 或距上次广播超过间隔时才广播，否则标记待补发。
    fn live(&mut self, apply: Box<dyn FnOnce(&mut StaffReply)>, forced: bool) {
        let applied = self
            .store
            .update_live(&self.scope.workspace_id, &self.task_id, |task| {
                let reply = task.running_reply_mut()?;
                apply(reply);
                Some(())
            });
        if !matches!(applied, Ok(Some(()))) {
            return;
        }
        if forced
            || self
                .last_sent
                .is_none_or(|at| at.elapsed() >= LIVE_INTERVAL)
        {
            self.last_sent = Some(Instant::now());
            self.dirty.store(false, Ordering::Relaxed);
            publish(&self.store, &self.scope, &self.task_id, &self.status);
        } else {
            self.dirty.store(true, Ordering::Relaxed);
            self.schedule_flush();
        }
    }

    fn schedule_flush(&self) {
        if self.flush_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let (store, scope, task_id, status) = (
            self.store.clone(),
            self.scope.clone(),
            self.task_id.clone(),
            self.status.clone(),
        );
        let (dirty, scheduled) = (self.dirty.clone(), self.flush_scheduled.clone());
        std::thread::spawn(move || {
            std::thread::sleep(LIVE_INTERVAL);
            scheduled.store(false, Ordering::Release);
            if dirty.swap(false, Ordering::AcqRel) {
                publish(&store, &scope, &task_id, &status);
            }
        });
    }

    fn save(&mut self, apply: Box<dyn FnOnce(&mut StaffReply)>) {
        let saved: std::io::Result<Task> =
            self.store
                .update(&self.scope.workspace_id, &self.task_id, |task| {
                    if let Some(reply) = task.running_reply_mut() {
                        apply(reply);
                    }
                    Ok(())
                });
        self.last_sent = Some(Instant::now());
        self.send(match saved {
            Ok(task) => BrainStatusKind::TaskChanged(task),
            Err(error) => BrainStatusKind::TaskError {
                reason: error.to_string(),
            },
        });
    }

    fn send(&self, kind: BrainStatusKind) {
        let _ = self.status.send(BrainStatus {
            scope: self.scope.clone(),
            kind,
        });
    }
}

/// 广播当前运行中回复的快照（只克隆这一条回复，不克隆整个任务）。
fn publish(store: &TaskStore, scope: &BrainScope, task_id: &str, status: &Sender<BrainStatus>) {
    let current = store.update_live(&scope.workspace_id, task_id, |task| {
        let index = task.messages.iter().rposition(
            |message| matches!(message, TaskMessage::Staff(reply) if reply.is_running()),
        )?;
        match &task.messages[index] {
            TaskMessage::Staff(reply) => Some((index, reply.clone())),
            _ => None,
        }
    });
    if let Ok(Some((index, reply))) = current {
        let _ = status.send(BrainStatus {
            scope: scope.clone(),
            kind: BrainStatusKind::TaskReplyChanged {
                task_id: task_id.to_owned(),
                index,
                reply,
            },
        });
    }
}
