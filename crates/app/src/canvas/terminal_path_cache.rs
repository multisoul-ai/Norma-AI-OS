//! 终端文件路径的存在性缓存：渲染路径只读缓存，后台批量 `fs::metadata` 验证，结果落地后刷新窗口。

use crate::cards::term::path_underline::PathMarks;
use crate::preview::terminal_path::resolve_terminal_path;
use gpui::App;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal::GridSnapshot;

pub(crate) const MAX_CANDIDATES_PER_ROUND: usize = 64;
pub(crate) const MISSING_TTL: Duration = Duration::from_secs(5);
const CHECK_DEBOUNCE: Duration = Duration::from_millis(150);
const IN_FLIGHT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_WANTED: usize = 512;
const MAX_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathState {
    Unknown,
    Pending,
    Missing,
    Exists { outside_project: bool },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PathKey {
    pub bases: Arc<[PathBuf]>,
    pub raw: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PathRequest {
    pub key: PathKey,
}

impl PathRequest {
    pub(crate) fn new(bases: Arc<[PathBuf]>, raw: impl Into<String>) -> Self {
        Self {
            key: PathKey {
                bases,
                raw: raw.into(),
            },
        }
    }
}

#[derive(Default)]
struct Entry {
    result: Option<(PathState, Instant)>,
    queued: bool,
    in_flight_since: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct TerminalPathCache {
    entries: HashMap<PathKey, Entry>,
    wanted: VecDeque<PathRequest>,
    version: u64,
    scheduled: bool,
}

impl TerminalPathCache {
    /// 展示用状态：Missing 过期重查期间仍报告 Missing，避免下划线闪烁。
    pub(crate) fn state(&self, key: &PathKey) -> PathState {
        match self.entries.get(key) {
            Some(Entry {
                result: Some((state, _)),
                ..
            }) => *state,
            Some(Entry {
                in_flight_since: Some(_),
                ..
            }) => PathState::Pending,
            _ => PathState::Unknown,
        }
    }

    /// 展示相关的状态变化计数；渲染缓存键包含它。
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    /// 登记一个可见候选；已有有效结果、已排队或检查中的不会重复排队。
    pub(crate) fn note(&mut self, request: PathRequest, now: Instant) {
        if self.wanted.len() >= MAX_WANTED {
            return;
        }
        if self.entries.len() > MAX_ENTRIES {
            self.purge();
        }
        let entry = self.entries.entry(request.key.clone()).or_default();
        if entry.queued {
            return;
        }
        if entry
            .in_flight_since
            .is_some_and(|since| now.duration_since(since) < IN_FLIGHT_TIMEOUT)
        {
            return;
        }
        match entry.result {
            Some((PathState::Missing, at)) if now.duration_since(at) >= MISSING_TTL => {}
            Some(_) => return,
            None => {}
        }
        entry.queued = true;
        self.wanted.push_back(request);
    }

    /// 先只丢不可见的 Missing 结果（无版本变化）；仍超限才清掉已确认的结果并升版本。
    fn purge(&mut self) {
        let busy = |entry: &Entry| entry.queued || entry.in_flight_since.is_some();
        self.entries.retain(|_, entry| {
            busy(entry) || !matches!(entry.result, Some((PathState::Missing, _)))
        });
        if self.entries.len() > MAX_ENTRIES {
            self.entries.retain(|_, entry| busy(entry));
            self.version += 1;
        }
    }

    /// 取出下一轮要检查的候选（最多 [`MAX_CANDIDATES_PER_ROUND`] 个）并标记为检查中。
    pub(crate) fn take_batch(&mut self, now: Instant) -> Vec<PathRequest> {
        let mut batch = Vec::new();
        while batch.len() < MAX_CANDIDATES_PER_ROUND {
            let Some(request) = self.wanted.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.get_mut(&request.key) {
                entry.queued = false;
                entry.in_flight_since = Some(now);
            }
            batch.push(request);
        }
        batch
    }

    /// 写入后台检查结果；展示状态有变化时递增版本并返回 true。
    pub(crate) fn apply(&mut self, results: Vec<(PathKey, PathState)>, now: Instant) -> bool {
        let mut changed = false;
        for (key, state) in results {
            let before = self.state(&key);
            let entry = self.entries.entry(key).or_default();
            entry.result = Some((state, now));
            entry.in_flight_since = None;
            changed |= before != state;
        }
        if changed {
            self.version += 1;
        }
        changed
    }
}

pub(crate) type TerminalPathCacheHandle = Rc<RefCell<TerminalPathCache>>;

/// 某个终端/Agent 卡片的路径验证范围：共享缓存 + 解析基准目录（Agent worktree、项目根）。
#[derive(Clone)]
pub(crate) struct TerminalPathScope {
    pub cache: TerminalPathCacheHandle,
    pub bases: Arc<[PathBuf]>,
}

impl TerminalPathScope {
    /// 与点击解析一致：Agent worktree 优先，其次项目根；没有项目时不验证路径。
    pub(crate) fn for_node(
        cache: &TerminalPathCacheHandle,
        runtime: &runtime::Runtime,
        node_id: canvas::NodeId,
    ) -> Option<Self> {
        let root = runtime.workspace_root()?;
        let mut bases: Vec<PathBuf> = Vec::with_capacity(2);
        if let Some(worktree) = runtime.agent_worktree(node_id) {
            bases.push(worktree.to_path_buf());
        }
        if !bases.iter().any(|base| base == root) {
            bases.push(root.to_path_buf());
        }
        Some(Self {
            cache: Rc::clone(cache),
            bases: bases.into(),
        })
    }

    /// 点击时只读缓存：后台已确认存在的候选才算已验证，与下划线显示的判定一致。
    pub(crate) fn is_verified(&self, raw: &str) -> bool {
        let key = PathKey {
            bases: Arc::clone(&self.bases),
            raw: raw.to_string(),
        };
        matches!(self.cache.borrow().state(&key), PathState::Exists { .. })
    }

    /// 仅在渲染缓存未命中时调用：读取缓存状态、登记新候选并生成下划线标记。
    pub(crate) fn marks_for(&self, snapshot: &GridSnapshot) -> PathMarks {
        let now = Instant::now();
        let mut cache = self.cache.borrow_mut();
        PathMarks::build(snapshot, |path_ref| {
            let request = PathRequest::new(Arc::clone(&self.bases), path_ref.raw.as_str());
            let state = cache.state(&request.key);
            cache.note(request, now);
            state
        })
    }
}

/// 后台线程调用：只做 `fs::metadata`，不扫描目录。
pub(crate) fn check_batch(
    batch: Vec<PathRequest>,
    home: Option<&Path>,
) -> Vec<(PathKey, PathState)> {
    batch
        .into_iter()
        .map(|request| {
            let state = classify(&request.key, home);
            (request.key, state)
        })
        .collect()
}

fn classify(key: &PathKey, home: Option<&Path>) -> PathState {
    match resolve_terminal_path(&key.bases, home, std::slice::from_ref(&key.raw)) {
        Ok(hit) => PathState::Exists {
            outside_project: is_outside_project(&hit.path, &key.bases),
        },
        Err(_) => PathState::Missing,
    }
}

fn is_outside_project(path: &Path, bases: &[PathBuf]) -> bool {
    let Ok(path) = path.canonicalize() else {
        return true;
    };
    !bases
        .iter()
        .filter_map(|base| base.canonicalize().ok())
        .any(|base| path.starts_with(base))
}

/// 去抖后在后台批量检查已登记的候选，结果落地后刷新窗口以重绘下划线。
pub(crate) fn kick_path_checks(cache: &TerminalPathCacheHandle, cx: &mut App) {
    {
        let mut state = cache.borrow_mut();
        if state.scheduled || state.wanted.is_empty() {
            return;
        }
        state.scheduled = true;
    }
    let cache = Rc::clone(cache);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    cx.spawn(async move |cx| {
        cx.background_executor().timer(CHECK_DEBOUNCE).await;
        loop {
            let batch = cache.borrow_mut().take_batch(Instant::now());
            if batch.is_empty() {
                break;
            }
            let home = home.clone();
            let results = cx
                .background_executor()
                .spawn(async move { check_batch(batch, home.as_deref()) })
                .await;
            let changed = cache.borrow_mut().apply(results, Instant::now());
            if changed && cx.refresh().is_err() {
                break;
            }
        }
        cache.borrow_mut().scheduled = false;
    })
    .detach();
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_path_cache_tests.rs"]
mod tests;
