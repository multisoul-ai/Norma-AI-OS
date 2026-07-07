use super::card_tasks::PendingCardTaskCallback;
use super::RealtimeControlState;
use crate::app::MultisoulApp;
use realtime::{RealtimeCallbackUpdate, RealtimeCommand, RealtimeToolResult};

impl RealtimeControlState {
    pub(super) fn queue_or_inject_tool_result(&mut self, result: RealtimeToolResult) {
        let update = self.session.queue_or_inject_tool_result(result);
        self.dispatch_tool_result_callbacks(update);
    }

    pub(super) fn can_accept_tool_result_callback(&self) -> bool {
        self.is_active() && self.has_sidecar()
    }

    pub(super) fn dispatch_tool_result_callbacks(&mut self, update: RealtimeCallbackUpdate) {
        if !update.dropped.is_empty() {
            tracing::warn!(
                dropped_callbacks = update.dropped.len(),
                "realtime callback queue dropped oldest tool result"
            );
        }

        for result in update.injected {
            if !self.can_accept_tool_result_callback() {
                tracing::info!(
                    "skipping realtime tool result injection because the session is unavailable"
                );
                continue;
            }
            if let Err(error) = self.send_sidecar_command(RealtimeCommand::ToolResult(result)) {
                tracing::warn!(%error, "failed to send realtime tool result");
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_tool_result_count_for_tests(&self) -> usize {
        self.session.pending_tool_result_count()
    }
}

impl MultisoulApp {
    pub(super) fn flush_realtime_card_task_callbacks(&mut self) {
        let callbacks = self.realtime.card_tasks.drain_completed_callbacks();
        if callbacks.is_empty() {
            return;
        }

        let Some(session_id) = self.realtime.current_session_id().cloned() else {
            tracing::info!(
                pending_callbacks = callbacks.len(),
                "skipping realtime tool result injection because no active session exists"
            );
            return;
        };

        for callback in callbacks {
            self.inject_card_task_callback(callback, &session_id);
        }
    }

    fn inject_card_task_callback(
        &mut self,
        callback: PendingCardTaskCallback,
        session_id: &realtime::RealtimeSessionId,
    ) {
        if !self.realtime.can_accept_tool_result_callback() {
            tracing::info!(
                task_id = %callback.task_id,
                "dropping realtime tool result callback because the session is unavailable"
            );
            return;
        }
        let Some(tool_call_id) = callback.tool_call_id else {
            tracing::info!(
                task_id = %callback.task_id,
                "skipping realtime tool result injection because tool_call_id is missing"
            );
            return;
        };
        self.realtime
            .queue_or_inject_tool_result(RealtimeToolResult::task_result(
                session_id.clone(),
                tool_call_id,
                callback.status,
                callback.task_id,
                callback.summary,
            ));
    }
}
