//! Native report delivery. Context fragments are acknowledged before commentary.
//! The protocol cannot identify which report owns output audio: never claim playback.
use crate::app::{shared_context::report::ReportDeliveryHandle, MultisoulApp};
use agent::context::ReportDelivery;
use live_voice::{AppendKind, LiveCommand, LiveError};
use norma::protocol::ReportPriority;
use std::collections::{HashMap, VecDeque};
use tokio_util::sync::CancellationToken;

pub(super) struct NativeReport {
    text: Option<String>,
    fragments: VecDeque<LiveCommand>,
    awaiting: HashMap<String, AppendKind>,
    commentary: Option<LiveCommand>,
    delivery: ReportDeliveryHandle,
    cancellation: CancellationToken,
    priority: ReportPriority,
    offered: bool,
    expires: std::time::Instant,
}

impl NativeReport {
    pub(super) fn new(
        text: String,
        priority: ReportPriority,
        delivery: ReportDeliveryHandle,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            text: Some(text),
            fragments: VecDeque::new(),
            awaiting: HashMap::new(),
            commentary: None,
            delivery,
            cancellation: cancellation.child_token(),
            priority,
            offered: false,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(15),
        }
    }

    // Called only by the background audio worker, never by GPUI foreground admission.
    fn prepare(&mut self) -> Result<(), String> {
        let text = self.text.take().ok_or("Live 报告已准备")?;
        let reference = self.delivery.reference();
        let encode = |part: usize, total: usize, text: &str| {
            serde_json::json!({
                "kind":"brain_report_fragment", "event_id":reference.event_id,
                "revision":reference.revision, "part":part, "total":total, "text":text,
            })
            .to_string()
        };
        let mut parts = Vec::new();
        let mut text_part = String::new();
        for ch in text.chars() {
            text_part.push(ch);
            // 64 parts is a hard cap. Its two digits reserve the final total field.
            if encode(parts.len() + 1, 64, &text_part).len() > 500 {
                text_part.pop();
                if text_part.is_empty() {
                    return Err("Live 报告引用超出单条预算".into());
                }
                parts.push(std::mem::take(&mut text_part));
                text_part.push(ch);
            }
        }
        if !text_part.is_empty() {
            parts.push(text_part);
        }
        if parts.is_empty() || parts.len() > 64 {
            return Err("Live 报告超出分段预算".into());
        }
        let total = parts.len();
        let command = |kind, content| LiveCommand::Append {
            event_id: format!("brain-report-{}", uuid::Uuid::new_v4()),
            kind,
            delegation_id: None,
            content,
        };
        self.fragments = parts
            .iter()
            .enumerate()
            .map(|(index, text)| command(AppendKind::Thinking, encode(index + 1, total, text)))
            .collect();
        if self.fragments.iter().any(
            |command| matches!(command, LiveCommand::Append { content, .. } if content.len() > 500),
        ) {
            return Err("Live 报告分段超出单条预算".into());
        }
        let content = serde_json::json!({"kind":"brain_report_ready", "event_id":reference.event_id, "revision":reference.revision, "total":total}).to_string();
        if content.len() > 500 {
            return Err("Live 报告引用超出单条预算".into());
        }
        self.commentary = Some(command(AppendKind::Commentary, content));
        Ok(())
    }

    fn send_next(
        &mut self,
        send: impl FnOnce(&LiveCommand, &CancellationToken) -> Result<(), LiveError>,
    ) -> Result<(), LiveError> {
        if !self.awaiting.is_empty() {
            return Ok(());
        }
        let next = self.fragments.front().or_else(|| {
            self.awaiting
                .is_empty()
                .then_some(self.commentary.as_ref())
                .flatten()
        });
        let Some(next) = next else {
            return Ok(());
        };
        let LiveCommand::Append { event_id, kind, .. } = next else {
            unreachable!()
        };
        match send(next, &self.cancellation) {
            Ok(()) => {
                self.offered = true;
                self.awaiting.insert(event_id.clone(), *kind);
                if self.fragments.pop_front().is_none() {
                    self.commentary = None;
                }
                Ok(())
            }
            Err(LiveError::Capacity) => Ok(()), // Retry admission only, never an accepted append.
            Err(error) => Err(error),
        }
    }

    fn acknowledge(&mut self, event_id: &str, kind: AppendKind) -> bool {
        if self.awaiting.get(event_id) != Some(&kind) {
            return false;
        }
        self.awaiting.remove(event_id);
        if self.fragments.is_empty() && self.awaiting.is_empty() && self.commentary.is_none() {
            // ACK proves transmission; local send_command only queues a transport write.
            self.delivery.set(ReportDelivery::Sent);
            self.delivery.set(ReportDelivery::Acknowledged);
            self.delivery.set(ReportDelivery::Unknown);
            return true;
        }
        false
    }
}

impl Drop for NativeReport {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if self.offered {
            self.delivery.set(ReportDelivery::Unknown);
        } else {
            self.delivery.cancel();
        }
    }
}

#[derive(Default)]
pub(super) struct NativeReports(VecDeque<NativeReport>);
impl NativeReports {
    pub fn push(&mut self, mut report: NativeReport) {
        if self.0.len() >= 8 {
            return;
        } // Dropped, unsent reports record Skipped.
        if report.cancellation.is_cancelled() {
            return;
        }
        if let Err(error) = report.prepare() {
            report.delivery.set(ReportDelivery::Failed);
            tracing::warn!(%error, "Live report could not be prepared");
            return;
        }
        match report.priority {
            ReportPriority::Now if self.0.front().is_some_and(|front| front.offered) => {
                self.0.insert(1, report)
            }
            ReportPriority::Now => self.0.push_front(report),
            ReportPriority::Normal => self.0.push_back(report),
        }
    }
    pub fn pump(
        &mut self,
        send: impl FnOnce(&LiveCommand, &CancellationToken) -> Result<(), LiveError>,
    ) -> Result<(), LiveError> {
        self.0.retain(|report| {
            !report.cancellation.is_cancelled() && std::time::Instant::now() < report.expires
        });
        if let Some(report) = self.0.front_mut() {
            match report.send_next(send) {
                // The foreground may cancel after retain but before transport admission.
                // That revokes this report only, not the call owning the transport.
                Err(LiveError::Cancelled) if report.cancellation.is_cancelled() => {
                    self.0.pop_front();
                }
                result => result?,
            }
        }
        Ok(())
    }
    pub fn acknowledge(&mut self, id: &str, kind: AppendKind) {
        self.0.retain_mut(|report| !report.acknowledge(id, kind));
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl MultisoulApp {
    pub(crate) fn queue_live_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        delivery: ReportDeliveryHandle,
    ) -> bool {
        if !self.gpt_live.can_report()
            || self.gpt_live.context_scope.as_ref() != Some(&delivery.reference().scope)
            || !self
                .shared_context_service()
                .is_ok_and(|service| service.scope() == &delivery.reference().scope)
        {
            delivery.cancel();
            return false;
        }
        let report = NativeReport::new(
            text,
            priority,
            delivery.clone(),
            self.gpt_live.report_cancellation.clone(),
        );
        delivery.set(ReportDelivery::Queued);
        self.gpt_live
            .reports
            .as_ref()
            .is_some_and(|reports| reports.try_send(report).is_ok())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_native_report_tests.rs"]
mod tests;
