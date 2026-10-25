//! 幕僚活动的后台持久化：前台只把条目放进发件箱，由外壳按周期取走、分批交给写入线程落库。
//!
//! 写入线程每次醒来把队列里的批次合并成一个事务；队列满时丢轮次正文并计数，动作退还给调用方重试。
//! 启动时与之后每 6 小时清理一次 7 天前的记录。

use super::log::AutoAction;
use brain::acp::trace::{AcpTrace, TraceKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};
use storage::{
    BrainActionRow, BrainActivityBatch, BrainActivityStore, BrainItemRow, BrainTurnUpsert,
};

const QUEUE_CAPACITY: usize = 256;
const FLUSH_EVERY: Duration = Duration::from_millis(500);
const PRUNE_EVERY: Duration = Duration::from_secs(6 * 3600);
/// 一次事务最多合并的批次数，避免一次持锁太久。
const MAX_MERGE: usize = 50;

/// 等着落库的一条记录。区域 id 由外壳在取走时补上，所以这里不带。
#[derive(Clone, Debug)]
pub(crate) enum Pending {
    Trace(AcpTrace),
    Action {
        action: AutoAction,
        /// 产生这个动作的（会话，轮次）；用户自己批准的派发没有。
        via: Option<(String, u64)>,
    },
}

pub(crate) fn unix_ms(at: SystemTime) -> i64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// 把一批记录转成存储行：同一轮在批内合并成一行，开始取最早一条，结束取 `Turn` 条目。
pub(crate) fn build_batch(
    launch_id: i64,
    area_id: &str,
    pending: Vec<Pending>,
) -> BrainActivityBatch {
    let mut batch = BrainActivityBatch::default();
    let mut turn_index: HashMap<(String, u64), usize> = HashMap::new();
    for entry in pending {
        match entry {
            Pending::Trace(trace) => {
                let at_ms = unix_ms(trace.at);
                let key = (trace.lane.clone(), trace.turn);
                let index = *turn_index.entry(key).or_insert_with(|| {
                    batch.turns.push(BrainTurnUpsert {
                        launch_id,
                        area_id: area_id.to_owned(),
                        lane: trace.lane.clone(),
                        turn: trace.turn as i64,
                        started_at_ms: at_ms,
                        trigger_kind: trace.trigger.kind.as_str().to_owned(),
                        trigger_who: trace.trigger.who.clone(),
                        finished_at_ms: None,
                        stop_reason: None,
                    });
                    batch.turns.len() - 1
                });
                let turn = &mut batch.turns[index];
                turn.started_at_ms = turn.started_at_ms.min(at_ms);
                if let Some(end) = &trace.item.end {
                    turn.finished_at_ms = Some(at_ms);
                    turn.stop_reason = Some(end.stop_reason.clone());
                }
                batch.items.push(BrainItemRow {
                    launch_id,
                    area_id: area_id.to_owned(),
                    lane: trace.lane,
                    turn: trace.turn as i64,
                    at_ms,
                    kind: kind_key(trace.item.kind).to_owned(),
                    title: trace.item.title,
                    text: trace.item.text,
                });
            }
            Pending::Action { action, via } => batch.actions.push(BrainActionRow {
                launch_id,
                area_id: area_id.to_owned(),
                at_ms: unix_ms(action.at),
                kind: action.kind.as_str().to_owned(),
                agent: action.agent,
                node_id: action.node_id as i64,
                summary: action.summary,
                reason: action.reason,
                ask_key: action.ask_key,
                via_lane: via.as_ref().map(|(lane, _)| lane.clone()),
                via_turn: via.map(|(_, turn)| turn as i64),
            }),
        }
    }
    batch
}

fn kind_key(kind: TraceKind) -> &'static str {
    match kind {
        TraceKind::Prompt => "prompt",
        TraceKind::Reply => "reply",
        TraceKind::Thought => "thought",
        TraceKind::ToolCall => "tool_call",
        TraceKind::ToolResult => "tool_result",
        TraceKind::Plan => "plan",
        TraceKind::Permission => "permission",
        TraceKind::Turn => "turn",
        TraceKind::Error => "error",
        TraceKind::Lifecycle => "lifecycle",
        TraceKind::Unknown => "unknown",
    }
}

/// `kind_key` 的逆运算；不认识的键退回 `Unknown`。
pub(crate) fn kind_from_key(key: &str) -> TraceKind {
    [
        TraceKind::Prompt,
        TraceKind::Reply,
        TraceKind::Thought,
        TraceKind::ToolCall,
        TraceKind::ToolResult,
        TraceKind::Plan,
        TraceKind::Permission,
        TraceKind::Turn,
        TraceKind::Error,
        TraceKind::Lifecycle,
    ]
    .into_iter()
    .find(|kind| kind_key(*kind) == key)
    .unwrap_or(TraceKind::Unknown)
}

enum Msg {
    Batch(BrainActivityBatch),
    Stop,
}

pub(crate) struct ActivityWriter {
    tx: SyncSender<Msg>,
    launch_id: i64,
    dropped: AtomicU64,
    thread: Mutex<Option<(JoinHandle<()>, Receiver<()>)>>,
}

impl ActivityWriter {
    pub(crate) fn start(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (done_tx, done_rx) = mpsc::channel();
        let store = BrainActivityStore::new(path);
        let handle = std::thread::spawn(move || {
            run(store, rx);
            let _ = done_tx.send(());
        });
        Self {
            tx,
            launch_id: unix_ms(SystemTime::now()),
            dropped: AtomicU64::new(0),
            thread: Mutex::new(Some((handle, done_rx))),
        }
    }

    /// 没有消费线程的写入器，队列只会被填满；给测试用。
    #[cfg(test)]
    pub(crate) fn detached(capacity: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(capacity);
        std::mem::forget(rx);
        Self {
            tx,
            launch_id: 1,
            dropped: AtomicU64::new(0),
            thread: Mutex::new(None),
        }
    }

    /// 本次启动的标识；轮次号每次启动从头计数，靠它区分。
    pub(crate) fn launch_id(&self) -> i64 {
        self.launch_id
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 非阻塞。队列满时丢掉轮次与正文并计数，返回只含动作的剩余，调用方下次重试。
    pub(crate) fn submit(&self, batch: BrainActivityBatch) -> Option<BrainActivityBatch> {
        if batch.is_empty() {
            return None;
        }
        match self.tx.try_send(Msg::Batch(batch)) {
            Ok(()) => None,
            Err(TrySendError::Full(Msg::Batch(batch)))
            | Err(TrySendError::Disconnected(Msg::Batch(batch))) => {
                // 轮次行由正文派生，按丢掉的正文条数计。
                self.dropped
                    .fetch_add(batch.items.len() as u64, Ordering::Relaxed);
                (!batch.actions.is_empty()).then(|| BrainActivityBatch {
                    actions: batch.actions,
                    ..Default::default()
                })
            }
            Err(_) => None,
        }
    }

    /// 通知线程写完队列后退出，最多等 `timeout`。返回是否在限时内完成。
    pub(crate) fn shutdown(&self, timeout: Duration) -> bool {
        let Some((handle, done)) = self.thread.lock().ok().and_then(|mut t| t.take()) else {
            return true;
        };
        let deadline = Instant::now() + timeout;
        while self.tx.try_send(Msg::Stop).is_err() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let finished = done
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_ok();
        if finished {
            let _ = handle.join();
        }
        finished
    }
}

static WRITER: OnceLock<ActivityWriter> = OnceLock::new();

/// 进程里唯一的写入器；第一次调用决定数据库路径。
pub(crate) fn shared(path: PathBuf) -> &'static ActivityWriter {
    WRITER.get_or_init(|| ActivityWriter::start(path))
}

/// 退出时调用：写入器启动过才等它写完，最多等 `timeout`，不会为了关闭再去启动它。
pub(crate) fn shutdown_shared(timeout: Duration) {
    if let Some(writer) = WRITER.get() {
        if writer.dropped() > 0 {
            tracing::warn!(
                dropped = writer.dropped(),
                "幕僚活动写入队列满过，丢了部分轮次正文"
            );
        }
        if !writer.shutdown(timeout) {
            tracing::warn!("幕僚活动退出时未能在限时内写完");
        }
    }
}

/// 发件箱最多攒这么多条；没人取走时丢最旧的，避免内存无限增长。
const OUTBOX_CAP: usize = 5000;

/// 各会话的待落库记录。内部可变，外壳只读取会话就能取走。
#[derive(Debug, Default)]
pub(crate) struct Outbox {
    pending: std::cell::RefCell<Vec<Pending>>,
}

impl Outbox {
    pub(crate) fn push(&self, entry: Pending) {
        let mut pending = self.pending.borrow_mut();
        pending.push(entry);
        if pending.len() > OUTBOX_CAP {
            let excess = pending.len() - OUTBOX_CAP;
            pending.drain(..excess);
        }
    }

    pub(crate) fn take(&self) -> Vec<Pending> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }
}

pub(crate) fn merge(into: &mut BrainActivityBatch, from: BrainActivityBatch) {
    absorb(into, from);
}

fn run(store: BrainActivityStore, rx: Receiver<Msg>) {
    prune(&store);
    let mut last_prune = Instant::now();
    loop {
        let mut merged = BrainActivityBatch::default();
        let mut stop = false;
        match rx.recv_timeout(FLUSH_EVERY) {
            Ok(Msg::Batch(batch)) => absorb(&mut merged, batch),
            Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => stop = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut count = 1;
        while !stop && count < MAX_MERGE {
            match rx.try_recv() {
                Ok(Msg::Batch(batch)) => absorb(&mut merged, batch),
                Ok(Msg::Stop) => stop = true,
                Err(_) => break,
            }
            count += 1;
        }
        if let Err(error) = store.save_batch(&merged) {
            tracing::warn!(%error, "幕僚活动写入失败");
        }
        if stop {
            return;
        }
        if last_prune.elapsed() >= PRUNE_EVERY {
            prune(&store);
            last_prune = Instant::now();
        }
    }
}

fn absorb(into: &mut BrainActivityBatch, from: BrainActivityBatch) {
    into.turns.extend(from.turns);
    into.items.extend(from.items);
    into.actions.extend(from.actions);
}

fn prune(store: &BrainActivityStore) {
    if let Err(error) = store.prune(unix_ms(SystemTime::now())) {
        tracing::warn!(%error, "幕僚活动清理失败");
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_activity_sink_tests.rs"]
mod brain_activity_sink_tests;
