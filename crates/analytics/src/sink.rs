//! 事件出口：`Sink` 抽象 + PostHog `/batch/` 实现。

use std::time::Duration;

use serde_json::{json, Value};

use crate::clock::iso8601;
use crate::queue::Queued;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// 已送达，可从队列移除。
    Sent,
    /// 暂时失败（网络 / 5xx / 429），保留并退避。
    Retry,
    /// 永久失败（其余 4xx），丢弃以免卡死队列。
    Drop,
}

pub trait Sink: Send {
    fn send(&mut self, batch: &[Queued]) -> SendOutcome;
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct PostHogSink {
    agent: ureq::Agent,
    url: String,
    project_key: String,
}

impl PostHogSink {
    pub fn new(host: &str, project_key: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // 10 秒限定连接 + 响应，避免卡住后台线程。
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .new_agent();
        Self {
            agent,
            url: format!("{}/batch/", host.trim_end_matches('/')),
            project_key: project_key.to_owned(),
        }
    }
}

/// 构造 PostHog `/batch/` 请求体。
pub fn batch_body(project_key: &str, batch: &[Queued]) -> Value {
    let events: Vec<Value> = batch
        .iter()
        .map(|item| {
            json!({
                "uuid": item.uuid,
                "event": item.event,
                "distinct_id": item.distinct_id,
                "timestamp": iso8601(item.timestamp_ms),
                "properties": item.properties,
            })
        })
        .collect();
    json!({ "api_key": project_key, "batch": events })
}

pub fn classify_status(status: u16) -> SendOutcome {
    match status {
        200..=299 => SendOutcome::Sent,
        408 | 429 | 500..=599 => SendOutcome::Retry,
        _ => SendOutcome::Drop,
    }
}

impl Sink for PostHogSink {
    fn send(&mut self, batch: &[Queued]) -> SendOutcome {
        let body = batch_body(&self.project_key, batch);
        match self.agent.post(&self.url).send_json(body) {
            Ok(response) => {
                let status = response.status().as_u16();
                tracing::debug!(status, "analytics PostHog response");
                classify_status(status)
            }
            Err(error) => {
                tracing::warn!(%error, "analytics PostHog request failed");
                SendOutcome::Retry
            }
        }
    }
}
