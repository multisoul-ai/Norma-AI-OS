//! 大脑下发的播报文字：沿用既有的播报队列、TTS 与豆包通道。

use super::{
    AgentAnnouncement, AgentAnnouncementTtsAvailability, MultisoulApp, PendingAgentAnnouncement,
    MAX_PENDING_ANNOUNCEMENTS,
};
use agent::context::{ContextPayload, DecisionStatus, ReceiptStatus};
use gpui::Context;
use norma::protocol::ReportPriority;
use sha2::{Digest, Sha256};

struct ConfirmationSource {
    operation_id: String,
    reason: String,
}

impl ConfirmationSource {
    fn receipt_id(&self) -> String {
        if self.operation_id.len() <= 256 {
            self.operation_id.clone()
        } else {
            // ApprovalTarget's individually valid fields can total more than 256 bytes.
            // Hash the full identity, never truncate a shared prefix.
            format!(
                "approval-sha256:{:x}",
                Sha256::digest(self.operation_id.as_bytes())
            )
        }
    }

    fn reason(&self) -> String {
        if self.reason.trim().is_empty() {
            "未提供理由".into()
        } else {
            self.reason.clone()
        }
    }
}

impl AgentAnnouncement {
    pub(super) fn report(text: String) -> Self {
        Self {
            node_id: 0,
            text,
            agent_name: "Norma".into(),
            outcome_word: "Report".into(),
            report: true,
            delivery: None,
        }
    }
}

impl MultisoulApp {
    /// Internal reminders share the same durable report ledger as CLI reports.
    /// Foreground work only captures scope; saving and delivery run asynchronously.
    pub(crate) fn speak_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        cx: &mut Context<Self>,
    ) {
        self.save_and_speak_brain_report(text, priority, None, cx);
    }

    /// The host already moved the tracked approval into its waiting-user state.
    /// Record that transfer; it is not a successful execution of the requested command.
    pub(crate) fn speak_brain_confirmation_report(
        &mut self,
        text: String,
        operation_id: String,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        self.save_and_speak_brain_report(
            text,
            ReportPriority::Now,
            Some(ConfirmationSource {
                operation_id,
                reason,
            }),
            cx,
        );
    }

    fn save_and_speak_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        confirmation: Option<ConfirmationSource>,
        cx: &mut Context<Self>,
    ) {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let service = match self.shared_context_service() {
            Ok(service) => service,
            Err(error) => {
                self.desktop_workspace.error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let report_id = uuid::Uuid::new_v4().to_string();
        cx.spawn(async move |this, cx| {
            let report_text = text.clone();
            let saved = service
                .run(None, move |access| {
                    let sources = match &confirmation {
                        Some(source) => {
                            let decision = access.host_event(
                                &format!("{report_id}:decision"),
                                ContextPayload::Decision {
                                    status: DecisionStatus::AwaitingConfirmation,
                                    reason: source.reason(),
                                },
                                vec![],
                            )?;
                            let receipt = access.host_event(
                                &format!("{report_id}:receipt"),
                                ContextPayload::Receipt {
                                    operation_id: source.receipt_id(),
                                    status: ReceiptStatus::Accepted,
                                    summary: format!("审批已转交用户确认，尚未放行或执行所请求操作。原审批标识：{}", source.operation_id),
                                },
                                vec![decision.reference.clone()],
                            )?;
                            vec![decision.reference, receipt.reference]
                        }
                        None => vec![],
                    };
                    access.report(&report_id, &report_text, sources)
                })
                .await;
            let record = match saved {
                Ok(record) => record,
                Err(error) => {
                    tracing::warn!(%error, "internal brain report was not saved or delivered");
                    let _ = this.update(cx, |app, cx| {
                        app.desktop_workspace.error =
                            Some(format!("保存幕僚提醒失败：{error}").into());
                        cx.notify();
                    });
                    return;
                }
            };
            let delivery = match service.report_delivery(record.reference) {
                Ok(delivery) => delivery,
                Err(error) => {
                    tracing::warn!(%error, "internal brain report delivery unavailable");
                    return;
                }
            };
            let current = this
                .update(cx, |app, cx| {
                    if !app
                        .shared_context_service()
                        .is_ok_and(|current| current.scope() == service.scope())
                    {
                        return false;
                    }
                    app.speak_saved_brain_report(text, priority, delivery.clone(), cx);
                    true
                })
                .unwrap_or(false);
            if !current {
                delivery.cancel();
            }
        })
        .detach();
    }

    pub(crate) fn speak_saved_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        delivery: super::super::shared_context::report::ReportDeliveryHandle,
        cx: &mut Context<Self>,
    ) -> bool {
        self.enqueue_brain_report(text, priority, Some(delivery), cx)
    }

    fn enqueue_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        delivery: Option<super::super::shared_context::report::ReportDeliveryHandle>,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = text.trim().to_string();
        let preview: String = text.chars().take(80).collect();
        if text.is_empty() || !self.brain_speaking() {
            tracing::info!(%preview, "大脑播报被跳过：文字为空或语音汇报未开启");
            if let Some(delivery) = &delivery {
                delivery.cancel();
            }
            return false;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            return delivery
                .is_some_and(|delivery| self.queue_live_brain_report(text, priority, delivery));
        }
        let availability = self.agent_announcement_tts_availability();
        if availability == AgentAnnouncementTtsAvailability::Unavailable {
            tracing::info!(%preview, "大脑播报被跳过：TTS 不可用");
            if let Some(delivery) = &delivery {
                delivery.cancel();
            }
            return false;
        }
        if let Some(delivery) = &delivery {
            delivery.set(agent::context::ReportDelivery::Queued);
        }
        let mut announcement = AgentAnnouncement::report(text);
        announcement.delivery = delivery;
        let pending = PendingAgentAnnouncement {
            announcement,
            waiting_for_tts: availability == AgentAnnouncementTtsAvailability::Waiting,
            deferred: availability == AgentAnnouncementTtsAvailability::Waiting,
            queued_at: std::time::Instant::now(),
        };
        let queue = &mut self.agent_announcements.pending;
        if queue.len() == MAX_PENDING_ANNOUNCEMENTS {
            queue.pop_front();
        }
        match priority {
            ReportPriority::Now => queue.push_front(pending),
            ReportPriority::Normal => queue.push_back(pending),
        }
        tracing::info!(%preview, ?priority, pending_count = queue.len(), "大脑播报已入队");
        self.start_next_agent_announcement(cx);
        true
    }
}
