use crate::app::MultisoulApp;
use canvas::{AgentProfile, CanvasNodeKind, NodeId};
use gpui::Context;
use std::path::Path;

#[cfg(test)]
#[path = "../../../tests/unit/mobile_agent_reopen_tests.rs"]
mod tests;

impl MultisoulApp {
    /// Stop only this turn; RPC acknowledgement or Escape alone never produces a successful receipt.
    pub(crate) fn interrupt_mobile_chat(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<serde_json::Value, String>>, &'static str> {
        if self.agent_hook_server.has_pending() {
            return Err("state_not_ready");
        }
        self.runtime.validate_mobile_interrupt(expected)?;
        let native = self
            .runtime
            .mobile_native_session(expected.node_id)
            .map_err(|_| "state_not_ready")?;
        let store = storage::MobileTurnStore::for_workspace(
            self.database_paths.app_db_path().to_path_buf(),
            self.workspace_id.clone().ok_or("state_not_ready")?,
        );
        let run = expected.run_id.as_deref().ok_or("state_not_ready")?;
        let turn = expected.turn_id.as_deref().ok_or("state_not_ready")?;
        let waiting = match expected.profile.as_str() {
            "Claude" => self.interrupt_mobile_terminal(expected.node_id, run, turn, cx),
            "Codex" if native.is_some() => {
                self.interrupt_mobile_terminal(expected.node_id, run, turn, cx)
            }
            "Codex" => self
                .runtime
                .interrupt_mobile_codex(expected.node_id, run, turn),
            _ => return Err("mobile_control_required"),
        }
        .map_err(|_| "outcome_unknown")?;
        let expected = expected.clone();
        let (reply, receipt) = async_channel::bounded(1);
        cx.spawn(async move |_, cx| {
            // 25s leaves 5s for durable receipt persistence within the gateway's 30s budget.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
            let mut outcome = loop {
                match waiting.try_recv() {
                    Ok(Ok(value)) => {
                        if value["turnId"].as_str() != expected.turn_id.as_deref()
                            || !matches!(value["status"].as_str(), Some("interrupted" | "completed" | "failed")) {
                            break Err("outcome_unknown".into());
                        }
                        break Ok(serde_json::json!({"node_id":expected.node_id,"run_id":expected.run_id,
                            "session_id":expected.session_id,"turn_id":expected.turn_id,"status":value["status"]}));
                    }
                    Ok(Err(error)) => break Err(error),
                    Err(crossbeam_channel::TryRecvError::Disconnected) => break Err("outcome_unknown".into()),
                    Err(crossbeam_channel::TryRecvError::Empty) => {}
                }
                if std::time::Instant::now() >= deadline { break Err("outcome_unknown".into()); }
                // 50ms = 20 nonblocking receipt checks/s only while awaiting this request.
                cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
            };
            if let (Some(saved), Ok(value)) = (native, &outcome) {
                let state = match value["status"].as_str() {
                    Some("interrupted") => storage::MobileTurnState::Interrupted,
                    Some("completed") => storage::MobileTurnState::Completed,
                    _ => storage::MobileTurnState::Failed,
                };
                let ended = storage::MobileTurnOutcome {
                    summary: None,
                    key: storage::MobileTurnKey {
                        node_id: expected.node_id, profile: expected.profile.clone(),
                        run_id: expected.run_id.clone().expect("validated run"),
                        session_id: expected.session_id.clone().expect("validated session"),
                        turn_id: expected.turn_id.clone().expect("validated turn"),
                    }, state,
                };
                let recorded = cx.background_executor().spawn(async move {
                    store.record_native_event(&saved.session, saved.transcript_path.as_deref(), Some(&ended))
                }).await;
                if recorded.is_err() { outcome = Err("outcome_unknown".into()); }
                else { crate::mobile::project_events::changed(); }
            }
            let _ = reply.try_send(outcome);
        }).detach();
        cx.notify();
        Ok(receipt)
    }

    /// Authorize project access before calling. Preparation and transcript polling stay off
    /// the GPUI foreground; the receipt only completes on a bound Provider result or an error.
    pub fn interrupt_mobile_terminal(
        &mut self,
        node_id: NodeId,
        run_id: &str,
        prompt_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<crossbeam_channel::Receiver<Result<serde_json::Value, String>>, String> {
        if self.agent_hook_server.has_pending() {
            return Err("请等待最新 Agent 状态同步后重试".into());
        }
        let (mut request, receipt) = self
            .runtime
            .interrupt_mobile_terminal(node_id, run_id, prompt_id)?;
        let preparation = cx.background_executor().spawn(async move {
            let result = request.prepare();
            (request, result)
        });
        cx.spawn(async move |this, cx| {
            let (mut request, prepared) = preparation.await;
            let sent = this
                .update(cx, |app, cx| {
                    let result = prepared.and_then(|()| {
                        if app.agent_hook_server.has_pending() {
                            return Err("准备期间 Agent 状态变化，请同步后重试".into());
                        }
                        app.runtime.send_mobile_terminal_interrupt(&mut request)
                    });
                    if let Err(error) = result {
                        app.runtime
                            .finish_mobile_terminal_interrupt(&request, Some(error));
                        cx.notify();
                        return false;
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !sent {
                return;
            }
            while request.is_active() {
                // 100ms = 10 bounded transcript reads/s while this one interrupt is pending.
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let (next, result) = cx
                    .background_executor()
                    .spawn(async move {
                        let result = request.poll();
                        (request, result)
                    })
                    .await;
                request = next;
                let keep_polling = this
                    .update(cx, |app, cx| {
                        // Let the normal frame loop consume queued Hooks and its coordination effects
                        // before accepting reader evidence that could otherwise end a newer turn.
                        if result.is_ok() && app.agent_hook_server.has_pending() {
                            return true;
                        }
                        if app
                            .runtime
                            .finish_mobile_terminal_interrupt(&request, result.err())
                        {
                            cx.notify();
                            return false;
                        }
                        request.is_active()
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
        Ok(receipt)
    }

    /// Explicitly replace an idle legacy PTY with a controlled session using the same history.
    /// Busy/queued work must finish first; authorization is checked by the calling gateway.
    pub fn reopen_mobile_agent(
        &mut self,
        node_id: NodeId,
        expected_session_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<String, String>>, String> {
        let definition = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        });
        let queued = self.pending_agent_submissions.has_pending_for_node(node_id)
            || self.session_reply_queue.has_pending_for_node(node_id)
            || definition.is_some_and(|id| {
                self.agent_task_coordinator
                    .as_ref()
                    .is_some_and(|coordinator| {
                        coordinator.is_busy(id) || coordinator.queued_task_count(id) > 0
                    })
            });
        if queued || self.agent_hook_server.has_pending() {
            return Err("请等待任务、排队消息和最新状态同步结束后再重开".into());
        }
        if self.persistence.project_store().is_none() {
            return Err("项目审计数据库不可用".into());
        }
        self.runtime
            .enable_mobile_agent_control(node_id, expected_session_id)?;
        // Persist opt-in even if Provider preparation fails. Retrying must not fall back to bypass.
        self.mark_session_dirty();
        cx.notify();
        self.start_mobile_agent(node_id, cx)
    }

    /// Project authorization precedes this seam. The caller receives a complete page from background IO.
    pub fn read_mobile_claude_history(
        &self,
        node_id: NodeId,
        run_id: &str,
        cursor: Option<norma::claude::TranscriptCursor>,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<norma::claude::TranscriptPage, String>>, String>
    {
        let request = self.runtime.mobile_claude_history(node_id, run_id)?;
        let (sender, receipt) = async_channel::bounded(1);
        cx.background_executor()
            .spawn(async move {
                // Match Codex history: 20 conversation records per request, without truncating message bodies.
                let _ = sender.try_send(request.read(cursor, 20));
            })
            .detach();
        Ok(receipt)
    }

    /// Explicit launch after card creation; the remote gateway must authorize its caller first.
    /// Dropping the receipt does not launch another session or cancel an accepted launch.
    pub fn start_mobile_agent(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<String, String>>, String> {
        let store = self
            .persistence
            .project_store()
            .ok_or("项目审计数据库不可用")?;
        let audit = storage::ApprovalAuditStore::new(store.path().to_path_buf()).with_turn_store(
            storage::MobileTurnStore::for_workspace(
                self.database_paths.app_db_path().to_path_buf(),
                self.workspace_id.clone().ok_or("workspace unavailable")?,
            ),
        );
        let launch = self
            .runtime
            .mobile_agent_launch(
                node_id,
                Path::new(norma::agent_hook::AGENT_HOOK_BINARY_NAME),
                80,
                24,
            )
            .map_err(|error| error.to_string())?
            .with_approval_audit(audit);
        Ok(self.prepare_mobile_agent(node_id, launch, cx))
    }

    /// Codex needs a real first task before its native TUI can resume the new history.
    pub fn start_mobile_codex(
        &mut self,
        node_id: NodeId,
        initial_task: String,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<String, String>>, String> {
        if !matches!(self.runtime.document.node(node_id).map(|node| &node.kind),
            Some(CanvasNodeKind::Agent(spec)) if spec.mobile_control && spec.profile == AgentProfile::Codex)
        {
            return Err("目标不是受控 Codex 卡片".into());
        }
        if initial_task.trim().is_empty() {
            return Err("请先输入任务".into());
        }
        let store = self
            .persistence
            .project_store()
            .ok_or("项目审计数据库不可用")?;
        let audit = storage::ApprovalAuditStore::new(store.path().to_path_buf()).with_turn_store(
            storage::MobileTurnStore::for_workspace(
                self.database_paths.app_db_path().to_path_buf(),
                self.workspace_id.clone().ok_or("workspace unavailable")?,
            ),
        );
        let launch = self
            .runtime
            .mobile_agent_launch(
                node_id,
                Path::new(norma::agent_hook::AGENT_HOOK_BINARY_NAME),
                80,
                24,
            )
            .map_err(|error| error.to_string())?
            .with_initial_task(initial_task)
            .with_approval_audit(audit);
        Ok(self.prepare_mobile_agent(node_id, launch, cx))
    }

    fn prepare_mobile_agent(
        &mut self,
        node_id: NodeId,
        launch: runtime::runtime::MobileAgentLaunch,
        cx: &mut Context<Self>,
    ) -> async_channel::Receiver<Result<String, String>> {
        let generation = launch.generation();
        let preparation = cx
            .background_executor()
            .spawn(async move { launch.prepare() });
        let (sender, receiver) = async_channel::bounded(1);
        cx.spawn(async move |this, cx| {
            let prepared = preparation.await;
            let outcome = this
                .update(cx, |app, cx| {
                    let result =
                        prepared.and_then(|prepared| app.runtime.attach_mobile_agent(prepared));
                    if let Err(error) = result {
                        app.runtime.cancel_mobile_agent_launch(node_id, generation);
                        return Err(error.to_string());
                    }
                    app.mark_session_dirty();
                    cx.notify();
                    app.runtime
                        .agent_session_id(node_id)
                        .ok_or_else(|| "受控会话未绑定".into())
                })
                .unwrap_or_else(|_| Err("目标工作区已关闭".into()));
            let _ = sender.try_send(outcome);
        })
        .detach();
        receiver
    }
}
