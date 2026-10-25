//! 把 `brain` 接到项目窗口：事件进、状态出、审批和播报的落地。

mod activity_feed;
mod activity_sink;
mod activity_view;
mod approvals;
mod away;
mod config;
mod history;
mod lifecycle;
mod log;
mod model_lists;
mod ops;
mod own_prompts;
mod panel;
mod panel_parts;
mod panel_rows;
mod proposal_actions;
pub(crate) mod proposals;
mod settings;
mod startup;
mod trace_state;

mod user_actions;
pub(crate) use config::{BrainSettings, SlowReminder, SummaryEvery};
pub(crate) use log::{ask_key, AutoAction, AutoKind, ASSIGN_KEY};
pub(crate) use model_lists::ModelList;
pub(crate) use settings::{adapter_package, BrainSettingsView};

use crate::app::MultisoulApp;
use brain::risk::ApprovalSubject;
use brain::{ApprovalRequest, BrainEvent, BrainService, BrainStatus, Outcome};
use gpui::Context;
use norma::mobile::ApprovalTarget;
use runtime::{AgentTaskState, RuntimeEvent};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "../../tests/unit/brain_host_tests.rs"]
mod brain_host_tests;

#[cfg(test)]
#[path = "../../tests/unit/brain_dispatch_tests.rs"]
mod brain_dispatch_tests;

const APPROVAL_POLL: Duration = Duration::from_millis(500);
const START_RETRY: Duration = Duration::from_secs(10);
const SLOW_AFTER: Duration = Duration::from_secs(60);

pub(crate) struct Tracked {
    pub(super) target: ApprovalTarget,
    pub(super) agent: String,
    pub(super) subject: ApprovalSubject,
    pub(super) escalated: bool,
    pub(super) since: Instant,
    pub(super) reminded: bool,
}

struct BrainHost {
    service: Option<BrainService>,
    status: Receiver<BrainStatus>,
    token: String,
}

impl Drop for BrainHost {
    fn drop(&mut self) {
        // 关闭会等线程退出，放到后台，避免阻塞前台。
        if let Some(service) = self.service.take() {
            std::thread::spawn(move || service.shutdown());
        }
    }
}

pub(crate) struct BrainHostState {
    settings: Option<config::BrainSettings>,
    settings_loading: bool,
    settings_revision: u64,
    start_generation: u64,
    starting: Option<u64>,
    pending_events: RefCell<VecDeque<BrainEvent>>,
    host: Option<BrainHost>,
    /// 已撤销令牌的服务仍在后台停止；排空其迟到记录后再释放通道。
    retiring: Vec<Receiver<BrainStatus>>,
    next_start: Instant,
    last_poll: Instant,
    tracked: HashMap<String, Tracked>,
    installed: Vec<brain::BackendKind>,
    installing: Option<brain::BackendKind>,
    /// 设置页列出的 Codex、OpenCode 模型。
    model_lists: HashMap<brain::BackendKind, ModelList>,
    install_error: Option<(brain::BackendKind, String)>,
    /// 工作空间窗口汇总各项目后的快照，概览渲染读它。
    pub(crate) overview: OverviewSnapshot,
    overview_at: Instant,
    /// 本项目里大脑与 ACP 适配器的往来、自主动作，先进发件箱，等外壳取走落库。
    activity_outbox: activity_sink::Outbox,
    /// 幕僚活动面板读到的内容与界面状态。
    pub(crate) activity: activity_view::ActivityView,
    /// 各会话最近一轮的轮次号，给动作标注“来自第几轮”。
    last_turn: HashMap<String, u64>,
    /// 外壳上一次提交被拒（队列满）而退回的动作，下次一起重试。
    activity_retry: storage::BrainActivityBatch,
    /// 大脑自己派出去的任务，不当作用户的目标。
    pub(crate) own_prompts: own_prompts::OwnPrompts,
    /// 大脑活动窗口是否打开（只在工作空间窗口上使用）。
    pub(crate) trace_open: bool,
    /// 窗口失焦与回来的跟踪，以及等着通知大脑的“回来”事件。
    pub(crate) away: away::AwayTracker,
    pub(crate) pending_return: Option<std::time::Duration>,
    /// 活动窗口自己的焦点：打开时抢走焦点，Esc 才不会被聚焦的终端吃掉。
    pub(crate) trace_focus: Option<gpui::FocusHandle>,
    /// 大脑提议、等用户确认的下一步；读过文件后 `proposals_path` 才有值。
    pub(crate) proposals: proposals::Proposals,
    proposals_path: Option<std::path::PathBuf>,
    proposals_loading: bool,
    proposals_ready: bool,
    proposals_retry_at: Instant,
    proposals_dirty: bool,
    proposals_save: Option<Receiver<Result<(), String>>>,
    proposal_writer: proposals::Writer,
    /// 概览里展开的提议：（项目区域 id，提议 id）。
    pub(crate) proposal_open: Option<(String, u64)>,
}

/// 概览需要的、分散在各项目会话里的大脑状态。
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OverviewSnapshot {
    /// 大脑正在判断的审批：（项目区域 id，卡片 id）。
    pub judging: HashSet<(String, u64)>,
    /// 至少一个项目里的大脑在运行；工作空间窗口自己不跑大脑，设置页靠它显示“运行中”。
    pub running: bool,
    /// 等用户确认的下一步，新的在前。
    pub proposals: Vec<OverviewProposal>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverviewProposal {
    pub area_id: String,
    pub proposal: proposals::Proposal,
    /// 目标 Agent 还没有卡片，派发时会新建。
    pub new_card: bool,
}

impl BrainHostState {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            settings: None,
            settings_loading: false,
            settings_revision: 0,
            start_generation: 0,
            starting: None,
            pending_events: RefCell::new(VecDeque::new()),
            host: None,
            retiring: Vec::new(),
            next_start: now,
            last_poll: now,
            tracked: HashMap::new(),
            installed: Vec::new(),
            installing: None,
            model_lists: HashMap::new(),
            install_error: None,
            overview: OverviewSnapshot::default(),
            overview_at: now,
            activity_outbox: activity_sink::Outbox::default(),
            activity: activity_view::ActivityView::default(),
            last_turn: HashMap::new(),
            activity_retry: storage::BrainActivityBatch::default(),
            own_prompts: own_prompts::OwnPrompts::default(),
            trace_open: false,
            away: away::AwayTracker::default(),
            pending_return: None,
            trace_focus: None,
            proposals: proposals::Proposals::default(),
            proposals_path: None,
            proposals_loading: false,
            proposals_ready: false,
            proposals_retry_at: now,
            proposals_dirty: false,
            proposals_save: None,
            proposal_writer: brain::persistence::shared_writer().clone(),
            proposal_open: None,
        }
    }

    pub(super) fn record_trace(&mut self, entry: brain::acp::trace::AcpTrace) {
        self.last_turn.insert(entry.lane.clone(), entry.turn);
        self.activity_outbox
            .push(activity_sink::Pending::Trace(entry));
    }

    /// 记一条自主动作，放进发件箱等待落库。放行与拒绝来自审批会话，派发来自主会话；
    /// 标注的是该会话最近一轮，是尽力而为的对应，用户自己批准的派发没有来源轮次。
    pub(super) fn record_action(&mut self, action: AutoAction) {
        let lane = match action.kind {
            AutoKind::Approved | AutoKind::Denied => Some("Approval"),
            AutoKind::Assigned => Some("Main"),
            AutoKind::UserDispatched => None,
        };
        let via = lane.and_then(|lane| {
            self.last_turn
                .get(lane)
                .map(|turn| (lane.to_owned(), *turn))
        });
        self.activity_outbox
            .push(activity_sink::Pending::Action { action, via });
    }

    pub(crate) fn token_matches(&self, token: &str) -> bool {
        !token.is_empty() && self.host.as_ref().is_some_and(|host| host.token == token)
    }
}

impl MultisoulApp {
    /// 每帧调用：按需启动大脑，转发事件，处理大脑的状态，并定期读取待处理审批。
    pub(crate) fn brain_tick(
        &mut self,
        events: &[RuntimeEvent],
        excerpts: &[(u64, Option<String>)],
        cx: &mut Context<Self>,
    ) {
        self.poll_brain_proposal_save(cx);
        self.brain.drain_retired();
        if self.brain.settings.is_none() {
            if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
                self.brain.settings = host.read(cx).brain.settings.clone();
            }
            if self.brain.settings.is_none() {
                self.refresh_brain_settings(cx);
            }
        }
        // 工作空间窗口没有大脑，但要汇总各项目的状态给概览。
        if self.brain.overview_at.elapsed() >= APPROVAL_POLL {
            self.brain.overview_at = Instant::now();
            self.refresh_brain_overview(cx);
            self.flush_brain_activity(cx);
            self.refresh_brain_activity(false, cx);
        }
        if let Some(away) = self.brain.pending_return.take() {
            let event = BrainEvent::UserReturned { away };
            for session in self.desktop_workspace.sessions.values() {
                session.read(cx).push_brain(event.clone());
            }
            self.push_brain(event);
        }
        if self.brain.settings.as_ref().is_some_and(|s| s.enabled) {
            self.load_brain_proposals(cx);
        }
        if self.brain.host.is_none() {
            self.try_start_brain(cx);
        }
        for event in events {
            self.forward_runtime_event(event, excerpts);
        }
        if self.brain.host.is_none() {
            return;
        }
        self.drain_brain_status(cx);
        if self.brain.last_poll.elapsed() >= APPROVAL_POLL {
            self.brain.last_poll = Instant::now();
            self.poll_approvals();
            self.remind_slow_judgements(cx);
        }
    }

    /// 退出时等后台写入线程把幕僚活动写完，最多等 1 秒。
    pub(crate) fn shutdown_brain_activity() {
        activity_sink::shutdown_shared(Duration::from_secs(1));
    }

    /// 外壳（或没有外壳时的自己）把各会话的待落库记录取走，交给后台写入线程。
    fn flush_brain_activity(&mut self, cx: &mut Context<Self>) {
        // 项目会话的记录由外壳统一取，避免重复写入。
        if self.desktop_host.is_some() {
            return;
        }
        let writer = activity_sink::shared(self.database_paths.app_db_path().to_path_buf());
        let launch = writer.launch_id();
        let mut batch = std::mem::take(&mut self.brain.activity_retry);
        activity_sink::merge(
            &mut batch,
            activity_sink::build_batch(launch, "", self.brain.activity_outbox.take()),
        );
        for ((_, area_id), session) in &self.desktop_workspace.sessions {
            let pending = session.read(cx).brain.activity_outbox.take();
            activity_sink::merge(
                &mut batch,
                activity_sink::build_batch(launch, area_id, pending),
            );
        }
        self.brain.activity_retry = writer.submit(batch).unwrap_or_default();
    }

    pub(super) fn push_brain(&self, event: BrainEvent) {
        if let Some(service) = self
            .brain
            .host
            .as_ref()
            .and_then(|host| host.service.as_ref())
        {
            service.push(event);
        } else if self
            .brain
            .settings
            .as_ref()
            .is_none_or(|settings| settings.enabled)
        {
            let mut pending = self.brain.pending_events.borrow_mut();
            pending.push_back(event);
            if pending.len() > 200 {
                pending.pop_front();
            }
        }
    }

    fn forward_runtime_event(&mut self, event: &RuntimeEvent, excerpts: &[(u64, Option<String>)]) {
        let RuntimeEvent::AgentTaskStateChanged { node_id, to, .. } = event else {
            return;
        };
        let Some(agent) = self.agent_announcement_name(*node_id) else {
            return;
        };
        let outcome = match to {
            AgentTaskState::Done => Outcome::Succeeded,
            AgentTaskState::Failed { .. } => Outcome::Failed,
            AgentTaskState::Thinking => {
                let prompt = self.runtime.agent_latest_prompt(*node_id);
                // 用户发给 Agent 的任务是大脑判断下一步的依据；大脑自己派的不算。
                if let Some(text) = prompt
                    .as_ref()
                    .filter(|t| !self.brain.own_prompts.contains(t))
                {
                    self.push_brain(BrainEvent::Goal {
                        agent: agent.clone(),
                        text: text.clone(),
                    });
                }
                let text = prompt.unwrap_or_else(|| "开始处理".into());
                self.push_brain(BrainEvent::AgentProgress { agent, text });
                return;
            }
            _ => return,
        };
        let excerpt = excerpts
            .iter()
            .find(|(id, _)| id == node_id)
            .and_then(|(_, text)| text.clone());
        // 拿不到 Agent 给的结果时退回用户发的话；这种摘要不参与关键词判断。
        let from_result = excerpt.is_some();
        let excerpt = excerpt
            .or_else(|| self.runtime.agent_latest_prompt(*node_id))
            .unwrap_or_default();
        self.push_brain(BrainEvent::AgentFinished {
            agent,
            outcome,
            excerpt,
            from_result,
        });
    }

    fn drain_brain_status(&mut self, cx: &mut Context<Self>) {
        while let Some(Ok(status)) = self.brain.host.as_ref().map(|host| host.status.try_recv()) {
            match status {
                BrainStatus::Ready => tracing::info!("大脑会话就绪"),
                BrainStatus::BackendDown { reason } => {
                    tracing::warn!(%reason, "大脑后端不可用");
                    let pending: Vec<String> = self.brain.tracked.keys().cloned().collect();
                    for id in pending {
                        self.escalate(&id, "幕僚暂时不可用", cx);
                    }
                }
                BrainStatus::Trace(entry) => self.brain.record_trace(entry),
                BrainStatus::ApprovalTurnEnded { id } => {
                    if self.brain.tracked.get(&id).is_some_and(|t| !t.escalated) {
                        self.escalate(&id, "我拿不准", cx);
                    }
                }
            }
        }
    }

    /// 把审批留给用户：它仍在“需要你处理”里，这里只负责开口提醒。
    pub(super) fn escalate(&mut self, id: &str, reason: &str, cx: &mut Context<Self>) {
        let Some(tracked) = self.brain.tracked.get_mut(id) else {
            return;
        };
        if tracked.escalated {
            return;
        }
        tracked.escalated = true;
        let text = format!(
            "需要你确认：{} 想执行 {}。{reason}。",
            tracked.agent,
            approvals::summarize(&tracked.subject)
        );
        self.speak_brain_report(text, norma::protocol::ReportPriority::Now, cx);
    }

    /// 判断超过 60 秒还没结论，按设置用语音提醒一次；概览里的“Norma 判断中”始终可见。
    fn remind_slow_judgements(&mut self, cx: &mut Context<Self>) {
        let voice = self
            .brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.slow_reminder == SlowReminder::VoiceAfter60s);
        let slow: Vec<String> = self
            .brain
            .tracked
            .iter()
            .filter(|(_, t)| !t.escalated && !t.reminded && t.since.elapsed() >= SLOW_AFTER)
            .map(|(id, _)| id.clone())
            .collect();
        for id in slow {
            let Some(tracked) = self.brain.tracked.get_mut(&id) else {
                continue;
            };
            tracked.reminded = true;
            if voice {
                let text = format!(
                    "{} 想执行 {}，我还在判断，你也可以在左边直接处理。",
                    tracked.agent,
                    approvals::summarize(&tracked.subject)
                );
                self.speak_brain_report(text, norma::protocol::ReportPriority::Normal, cx);
            }
        }
    }

    fn poll_approvals(&mut self) {
        let Some(root) = self.runtime.workspace_root().map(|root| root.to_path_buf()) else {
            return;
        };
        let waiting: Vec<u64> = self
            .runtime
            .mobile_card_snapshots()
            .into_iter()
            .filter(|card| card.mobile_control && card.state == "waiting_approval")
            .map(|card| card.node_id)
            .collect();
        let mut seen = HashSet::new();
        for node_id in waiting {
            let Ok(snapshots) = self.runtime.mobile_approval_snapshots(node_id) else {
                continue;
            };
            for snapshot in snapshots.into_iter().filter(|s| !s.decisions.is_empty()) {
                let id = approvals::encode_id(&snapshot.target);
                seen.insert(id.clone());
                if self.brain.tracked.contains_key(&id) {
                    continue;
                }
                let subject = approvals::subject_of(&snapshot, &root);
                let agent = self
                    .agent_announcement_name(node_id)
                    .unwrap_or_else(|| snapshot.card_title.clone());
                self.brain.tracked.insert(
                    id.clone(),
                    Tracked {
                        target: snapshot.target.clone(),
                        agent: agent.clone(),
                        subject: subject.clone(),
                        escalated: false,
                        since: Instant::now(),
                        reminded: false,
                    },
                );
                self.push_brain(BrainEvent::ApprovalRequested(ApprovalRequest {
                    id,
                    agent,
                    subject,
                }));
            }
        }
        let closed: Vec<String> = self
            .brain
            .tracked
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in closed {
            self.brain.tracked.remove(&id);
            self.push_brain(BrainEvent::ApprovalClosed { id });
        }
    }
}
