//! 产品分析：强类型事件 + 离线队列 + PostHog 批量发送。
//!
//! 域层 crate：不依赖 GPUI / Storage / Keychain。热路径（`track` / `touch`）只做
//! 内存操作和 `mpsc::send`，网络与磁盘 IO 全在后台线程。构建时没有
//! `NORMA_POSTHOG_KEY` 则整体为 no-op。

mod clock;
mod config;
mod debounce;
mod event;
mod identity;
mod pipeline;
mod props;
mod queue;
mod sink;
mod timer;
mod token;
mod worker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use uuid::Uuid;

pub use clock::iso8601;
pub use config::{opt_out_marker, Config, DEFAULT_HOST};
pub use debounce::{Debouncer, FEATURE_WINDOW_MS};
pub use event::Event;
pub use identity::{hash_user_id, Identity};
pub use pipeline::{Pipeline, BATCH_SIZE, SEND_INTERVAL_MS};
pub use props::*;
pub use queue::{DiskQueue, Queued, MAX_QUEUED};
pub use sink::{batch_body, classify_status, PostHogSink, SendOutcome, Sink};
pub use timer::{ActiveTimer, IDLE_WINDOW_MS};
pub use token::Token;

use worker::Msg;

static GLOBAL: OnceLock<Analytics> = OnceLock::new();

/// 初始化全局分析句柄（仅首次生效），返回该句柄。
pub fn init(config: Config) -> Analytics {
    GLOBAL.get_or_init(|| Analytics::start(config)).clone()
}

/// 全局上报；未初始化或未启用时为 no-op。
pub fn track(event: Event) {
    if let Some(analytics) = GLOBAL.get() {
        analytics.track(event);
    }
}

/// 全局交互心跳：仅更新内存计时。
pub fn touch() {
    if let Some(analytics) = GLOBAL.get() {
        analytics.touch();
    }
}

pub fn global() -> Option<&'static Analytics> {
    GLOBAL.get()
}

#[derive(Clone)]
pub struct Analytics {
    inner: Option<Arc<Inner>>,
}

struct Inner {
    tx: Sender<Msg>,
    enabled: AtomicBool,
    session_id: String,
    app_version: String,
    id_salt: String,
    origin: Instant,
    state: Mutex<State>,
}

struct State {
    identity: Identity,
    timer: ActiveTimer,
    features: Debouncer<Feature>,
}

impl Analytics {
    /// 没有 project key 的构建返回 no-op 句柄。
    pub fn start(config: Config) -> Self {
        if !config.has_project_key() {
            return Self::disabled();
        }
        let sink = PostHogSink::new(&config.host, &config.project_key);
        Self::start_with_sink(config, sink)
    }

    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// 注入 Sink，供测试使用。
    pub fn start_with_sink<S: Sink + 'static>(config: Config, sink: S) -> Self {
        let store = DiskQueue::new(config.data_dir.join("analytics_queue.jsonl"));
        let pipeline = Pipeline::new(sink, store, config.enabled);
        let (tx, rx) = channel();
        let marker = opt_out_marker(&config.data_dir);
        let spawned = std::thread::Builder::new()
            .name("analytics".into())
            .spawn(move || worker::run(pipeline, rx, marker));
        if spawned.is_err() {
            return Self::disabled();
        }
        let inner = Inner {
            tx,
            enabled: AtomicBool::new(config.enabled),
            session_id: Uuid::new_v4().to_string(),
            app_version: config.app_version.clone(),
            id_salt: config.id_salt.clone(),
            origin: Instant::now(),
            state: Mutex::new(State {
                identity: Identity::load(&config.data_dir),
                timer: ActiveTimer::default(),
                features: Debouncer::default(),
            }),
        };
        Self {
            inner: Some(Arc::new(inner)),
        }
    }

    /// 构建带有 project key 且后台线程已启动；否则设置页不应显示开关。
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    pub fn is_enabled(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|i| i.enabled.load(Ordering::Relaxed))
    }

    pub fn track(&self, event: Event) {
        if let Some(inner) = &self.inner {
            inner.track(event);
        }
    }

    /// 键鼠 / 功能交互心跳，用于活跃时长。
    pub fn touch(&self) {
        if let Some(inner) = self
            .inner
            .as_ref()
            .filter(|i| i.enabled.load(Ordering::Relaxed))
        {
            let now = inner.mono_ms();
            if let Ok(mut state) = inner.state.lock() {
                state.timer.touch(now);
            }
        }
    }

    pub fn on_foreground(&self) {
        if let Some(inner) = self
            .inner
            .as_ref()
            .filter(|i| i.enabled.load(Ordering::Relaxed))
        {
            let now = inner.mono_ms();
            let started = inner.state.lock().is_ok_and(|mut s| {
                let was = s.timer.is_foreground();
                s.timer.foreground(now);
                !was
            });
            if started {
                inner.track(Event::AppForegrounded);
            }
        }
    }

    pub fn on_background(&self) {
        if let Some(inner) = self
            .inner
            .as_ref()
            .filter(|i| i.enabled.load(Ordering::Relaxed))
        {
            let now = inner.mono_ms();
            let closed = inner
                .state
                .lock()
                .ok()
                .and_then(|mut s| s.timer.background(now));
            if let Some((foreground_ms, active_ms)) = closed {
                inner.track(Event::AppBackgrounded {
                    foreground_ms,
                    active_ms,
                });
            }
        }
    }

    /// 授权成功后调用：用盐哈希 `raw` 并把匿名历史并入该用户。
    pub fn identify(&self, raw: &str) {
        let Some(inner) = &self.inner else { return };
        if !inner.enabled.load(Ordering::Relaxed) {
            return;
        }
        let change = inner.state.lock().ok().and_then(|mut s| {
            let anon = s.identity.anonymous_id().to_owned();
            s.identity
                .identify(raw, &inner.id_salt)
                .map(|id| (id, anon))
        });
        if let Some((distinct_id, anon)) = change {
            let mut props = serde_json::Map::new();
            props.insert("$anon_distinct_id".into(), Value::String(anon));
            inner.enqueue("$identify", distinct_id, props);
        }
    }

    /// 退出登录：换新的匿名身份。
    pub fn reset(&self) {
        if let Some(inner) = &self.inner {
            if let Ok(mut state) = inner.state.lock() {
                state.identity.reset();
            }
        }
    }

    /// 用户开关。关闭时先尽力发出 `analytics_opted_out`，随后清空内存与磁盘队列。
    pub fn set_enabled(&self, enabled: bool) {
        let Some(inner) = &self.inner else { return };
        let was = inner.enabled.swap(enabled, Ordering::Relaxed);
        if was && !enabled {
            // 开关已置 false，track 会被拦截，因此直接入队。
            let distinct_id = inner.distinct_id();
            inner.enqueue("analytics_opted_out", distinct_id, serde_json::Map::new());
            let _ = inner.tx.send(Msg::OptOut);
        } else if !was && enabled {
            let _ = inner.tx.send(Msg::SetEnabled(true));
        }
    }

    /// 仅用于退出路径：最多等待 `timeout` 让后台线程尽力发送。
    pub fn flush_blocking(&self, timeout: Duration) {
        let Some(inner) = &self.inner else { return };
        let (done_tx, done_rx) = channel();
        if inner.tx.send(Msg::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv_timeout(timeout);
        }
    }
}

impl Inner {
    fn mono_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    fn distinct_id(&self) -> String {
        self.state
            .lock()
            .map(|s| s.identity.distinct_id().to_owned())
            .unwrap_or_default()
    }

    fn track(&self, event: Event) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let now = self.mono_ms();
        let distinct_id = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.timer.touch(now);
            if let Event::FeatureUsed { feature, .. } = &event {
                if !state.features.allow(*feature, now, FEATURE_WINDOW_MS) {
                    return;
                }
            }
            state.identity.distinct_id().to_owned()
        };
        self.enqueue(event.name(), distinct_id, event.properties());
    }

    fn enqueue(
        &self,
        name: &str,
        distinct_id: String,
        mut properties: serde_json::Map<String, Value>,
    ) {
        properties.insert("session_id".into(), Value::String(self.session_id.clone()));
        properties.insert("$session_id".into(), Value::String(self.session_id.clone()));
        properties
            .entry("app_version")
            .or_insert_with(|| Value::String(self.app_version.clone()));
        properties.insert("$lib".into(), Value::String("norma-analytics".into()));
        let item = Queued {
            uuid: Uuid::new_v4().to_string(),
            event: name.to_owned(),
            distinct_id,
            timestamp_ms: clock::unix_ms(),
            properties,
        };
        let _ = self.tx.send(Msg::Event(item));
    }
}
