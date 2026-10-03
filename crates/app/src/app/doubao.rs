use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

mod plan;
mod playback;
mod tools;

use plan::run_session;

fn connect_failure_message(error: &tokio_tungstenite::tungstenite::Error) -> &'static str {
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401 => {
            "方舟 API Key 未通过 ASR 2.0 鉴权；请检查 Key 与资源权限"
        }
        tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 403 => {
            "方舟账号尚未获得 ASR 2.0 资源权限"
        }
        _ => "豆包 ASR 2.0 连接失败，请检查网络和资源权限",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DoubaoStatus {
    Idle,
    Connecting,
    Connected,
    Thinking,
    Speaking,
    Disconnected(String),
}

pub(crate) struct DoubaoVoiceState {
    pub(crate) status: DoubaoStatus,
    pub(crate) generation: u64,
    pub(crate) cancellation: Option<CancellationToken>,
    session_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) reports: Option<async_channel::Sender<String>>,
    pub(crate) tracked_tasks: HashSet<u64>,
}

impl Default for DoubaoVoiceState {
    fn default() -> Self {
        Self {
            status: DoubaoStatus::Idle,
            generation: 0,
            cancellation: None,
            session_task: None,
            reports: None,
            tracked_tasks: HashSet::new(),
        }
    }
}

impl DoubaoVoiceState {
    pub(crate) fn stop(&mut self) {
        if self.is_active() {
            tracing::info!(target: "norma_voice", generation = self.generation,
                "Doubao session cancellation requested");
        }
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.reports = None;
        self.tracked_tasks.clear();
        self.status = DoubaoStatus::Idle;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }
}

impl MultisoulApp {
    pub(crate) fn queue_doubao_terminal_reports(&mut self) {
        let (Some(sender), Some(coordinator)) = (
            self.doubao.reports.as_ref(),
            self.agent_task_coordinator.as_ref(),
        ) else {
            return;
        };
        for task in coordinator.tasks() {
            if !self.doubao.tracked_tasks.contains(&task.task_id) {
                continue;
            }
            let Some(report) = task_completion_message(&task) else {
                continue;
            };
            if sender.try_send(report).is_ok() {
                self.doubao.tracked_tasks.remove(&task.task_id);
            }
        }
    }

    pub(crate) fn stop_doubao_voice(&mut self, cx: &mut Context<Self>) {
        self.doubao.stop();
        cx.notify();
    }

    pub(crate) fn toggle_doubao_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doubao.is_active() {
            self.stop_doubao_voice(cx);
            return;
        }
        let Some(account) = self
            .voice_settings
            .values()
            .doubao_credential_ref()
            .map(str::to_owned)
        else {
            self.doubao.status =
                DoubaoStatus::Disconnected("请在 Voice Setup 保存豆包 API Key".into());
            cx.notify();
            return;
        };
        let canvas = self
            .current_canvas_context_for_provider()
            .unwrap_or_else(|| agent::CurrentCanvasContext {
                project_name: "未打开项目".into(),
                workspace_root: "未打开项目；项目操作不可用".into(),
                total_cards: 0,
                cards: Vec::new(),
                conversation_tails: Vec::new(),
            });
        let project_id = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        let bound_project = project_id.clone();
        let roster = self.agent_roster_for_provider();
        let prompt = agent::norma_system_prompt(&canvas, &roster);
        self.doubao.generation = self.doubao.generation.wrapping_add(1);
        let generation = self.doubao.generation;
        self.doubao.tracked_tasks.clear();
        let tool_adapter = super::agent_runtime::start_doubao_adapter(
            project_id.clone().unwrap_or_default(),
            generation,
            self.database_paths.clone(),
            self.workspace_state.current_root.clone(),
            window,
            cx,
        );
        let tools = Some(agent::NormaToolRuntime::new(tool_adapter));
        let database = self.database_paths.app_db_path().to_path_buf();
        let cancellation = CancellationToken::new();
        self.doubao.cancellation = Some(cancellation.clone());
        self.doubao.status = DoubaoStatus::Connecting;
        tracing::info!(target: "norma_voice", generation,
            "Doubao session start requested");
        let (events_tx, events_rx) = async_channel::bounded(32);
        let (reports_tx, reports_rx) = async_channel::unbounded();
        self.doubao.reports = Some(reports_tx);
        let playback_events = self
            .voice_window_inset_events
            .sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao);
        if let Ok(runtime) = super::shared_tokio_runtime() {
            let previous_task = self.doubao.session_task.take();
            self.doubao.session_task = Some(runtime.spawn(async move {
                if let Some(previous_task) = previous_task {
                    tracing::info!(target: "norma_voice", generation,
                        "waiting for previous Doubao session to finish");
                    let _ = previous_task.await;
                    tracing::info!(target: "norma_voice", generation,
                        "previous Doubao session finished");
                }
                if cancellation.is_cancelled() {
                    return;
                }
                let result = run_session(
                    account,
                    prompt,
                    project_id,
                    database,
                    tools,
                    cancellation,
                    (events_tx.clone(), reports_rx, playback_events),
                )
                .await;
                tracing::info!(target: "norma_voice", generation,
                    reason = result.as_ref().err().map(String::as_str).unwrap_or("normal"),
                    "Doubao session task finished");
                let _ = events_tx
                    .send(DoubaoStatus::Disconnected(
                        result.err().unwrap_or_else(|| "通话已结束".into()),
                    ))
                    .await;
            }));
        } else {
            self.doubao.stop();
            self.doubao.status = DoubaoStatus::Disconnected("无法启动语音网络运行时".into());
            cx.notify();
            return;
        }
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let still_active = this
                .update_in(cx, |app, _, cx| {
                    if app.doubao.generation != generation {
                        return false;
                    }
                    let now = app
                        .runtime
                        .workspace_root()
                        .map(|root| root.display().to_string());
                    if now != bound_project {
                        app.stop_doubao_voice(cx);
                        return false;
                    }
                    while let Ok(status) = events_rx.try_recv() {
                        let ended = matches!(status, DoubaoStatus::Disconnected(_));
                        if let DoubaoStatus::Disconnected(message) = &status {
                            tracing::info!(target: "norma_voice", reason = %message, "voice conversation ended");
                        }
                        app.doubao.status = status;
                        if ended {
                            app.doubao.cancellation = None;
                            app.doubao.reports = None;
                            app.doubao.tracked_tasks.clear();
                        }
                        cx.notify();
                    }
                    app.doubao.is_active()
                })
                .unwrap_or(false);
            if !still_active {
                break;
            }
        })
        .detach();
        cx.notify();
    }
}

fn task_completion_message(task: &agent::AgentCoordinatedTask) -> Option<String> {
    let outcome = match task.state {
        "succeeded" => "已完成",
        "failed" => "失败",
        "cancelled" => "已取消",
        "interrupted" => "已中断",
        _ => return None,
    };
    Some(format!(
        "{} 的任务 #{} {}。",
        task.agent, task.task_id, outcome
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/doubao_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_visual_tests.rs"]
mod visual_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_memory_tests.rs"]
mod memory_tests;
