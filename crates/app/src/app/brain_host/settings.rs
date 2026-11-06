//! 设置页对大脑的读写：配置落盘、重启大脑、安装适配器。

use super::config::{self, Autonomy, BrainSettings};
use crate::app::MultisoulApp;
use brain::BackendKind;
use gpui::Context;

/// 设置页渲染所需的快照。后端检测结果有缓存，渲染时不做文件 IO。
#[derive(Clone)]
pub(crate) struct BrainSettingsView {
    pub settings: BrainSettings,
    pub running: bool,
    pub installed: Vec<BackendKind>,
    /// 正在安装的后端。
    pub installing: Option<BackendKind>,
    /// 上一次安装失败的后端与原因。
    pub install_error: Option<(BackendKind, String)>,
    /// Codex、OpenCode 可选的模型。
    pub model_lists: std::collections::HashMap<BackendKind, super::ModelList>,
    /// “清除幕僚活动记录”已点过一次，等待确认。
    pub confirm_clear_activity: bool,
}

impl BrainSettingsView {
    /// 实际使用的后端：用户选的；没选时取第一个已安装的。
    pub(crate) fn chosen_backend(&self) -> Option<BackendKind> {
        self.settings
            .backend
            .or_else(|| self.installed.first().copied())
    }
}

/// 能一键装进 `~/.norma/brain/adapters` 的 ACP 适配器；OpenCode 的 ACP 由它自己的 CLI 提供。
pub(crate) fn adapter_package(kind: BackendKind) -> Option<&'static str> {
    match kind {
        BackendKind::ClaudeCode => Some("@agentclientprotocol/claude-agent-acp@0.94.0"),
        BackendKind::Codex => Some("@agentclientprotocol/codex-acp@2.2.2"),
        BackendKind::OpenCode => None,
    }
}

impl MultisoulApp {
    /// 大脑正在判断（还没升级给用户）的审批所属的 Agent 卡片。
    pub(crate) fn brain_judging_nodes(&self) -> std::collections::HashSet<u64> {
        self.brain
            .tracked
            .values()
            .filter(|tracked| !tracked.escalated)
            .map(|tracked| tracked.target.node_id)
            .collect()
    }

    /// 启用幕僚后，概览才显示“Norma 自主处理”栏。
    pub(crate) fn brain_enabled(&self) -> bool {
        self.brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.enabled)
    }

    pub(crate) fn brain_speaking(&self) -> bool {
        self.brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.enabled && settings.speak)
    }

    /// 只改“是否出声”，不重启大脑：关闭后大脑继续处理审批和派活。
    pub(crate) fn set_brain_speaking(&mut self, speak: bool, cx: &mut Context<Self>) {
        self.update_brain_settings_with(move |s| s.speak = speak, false, cx);
    }

    pub(crate) fn brain_autonomy(&self) -> Autonomy {
        self.brain.settings.clone().unwrap_or_default().autonomy
    }

    pub(crate) fn brain_settings_view(&self) -> BrainSettingsView {
        BrainSettingsView {
            settings: self.brain.settings.clone().unwrap_or_default(),
            running: self.brain.is_running() || self.brain.overview.running,
            installed: self.brain.installed.clone(),
            installing: self.brain.installing,
            install_error: self.brain.install_error.clone(),
            model_lists: self.brain.model_lists.clone(),
            confirm_clear_activity: self.brain.activity.confirm_clear,
        }
    }

    /// 打开设置页时调用：读一次配置并刷新已安装的后端（后台线程，不阻塞前台）。
    pub(crate) fn refresh_brain_settings(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = config::state_dir() else {
            self.desktop_workspace.error = Some(ui::i18n::message(
                "chrome.brain_data_directory_missing",
                &[],
            ));
            return;
        };
        self.refresh_brain_settings_from(directory, cx);
    }

    pub(super) fn refresh_brain_settings_from(
        &mut self,
        directory: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.brain.settings_loading {
            return;
        }
        let owner = super::owner::shared(cx);
        let preparing = owner.begin_prepare(std::time::Instant::now());
        if !preparing && owner.directory().is_none() {
            if let Some(reason) = owner.preparation_error() {
                self.desktop_workspace.error = Some(ui::i18n::message(
                    "chrome.brain_data_failed",
                    &[("reason", reason)],
                ));
            }
            return;
        }
        self.brain.settings_loading = true;
        let database = self.database_paths.app_db_path().to_path_buf();
        let sources = owner.sources();
        let work = cx.background_executor().spawn(async move {
            let prepared = preparing.then(|| super::data::prepare(&directory, database, sources));
            let backends =
                brain::backend::detect(&directory.join("adapters"), &config::search_dirs());
            (prepared, backends)
        });
        cx.spawn(async move |this, cx| {
            let (prepared, backends) = work.await;
            // The initiating window can close; data readiness belongs to the App owner.
            if let Some(result) = prepared {
                owner.finish_prepare(result);
            }
            let _ = this.update(cx, |app, cx| {
                app.brain.settings_loading = false;
                if owner.directory().is_some() {
                    app.brain.settings = owner.settings();
                } else if let Some(reason) = owner.preparation_error() {
                    app.desktop_workspace.error = Some(ui::i18n::message(
                        "chrome.brain_data_failed",
                        &[("reason", reason)],
                    ));
                }
                app.brain.installed = backends.iter().map(|backend| backend.kind).collect();
                app.discover_brain_models(backends, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// 修改并保存设置，然后丢弃正在运行的大脑，下一帧按新配置重新启动。
    pub(crate) fn update_brain_settings(
        &mut self,
        change: impl FnOnce(&mut BrainSettings),
        cx: &mut Context<Self>,
    ) {
        self.update_brain_settings_with(change, true, cx);
    }

    /// 只有工作空间窗口保存配置；各项目会话只接收，避免各自保存互相覆盖。
    pub(crate) fn update_brain_settings_with(
        &mut self,
        change: impl FnOnce(&mut BrainSettings),
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        let owner = super::owner::shared(cx);
        let Some((directory, mut settings)) = owner.directory().zip(owner.settings()) else {
            self.desktop_workspace.error =
                Some(ui::i18n::message("chrome.brain_data_not_ready", &[]));
            cx.notify();
            return;
        };
        change(&mut settings);
        self.apply_brain_settings(settings.clone(), restart, cx);
        let sessions: Vec<_> = self.desktop_workspace.sessions.values().cloned().collect();
        for session in sessions {
            let copy = settings.clone();
            session.update(cx, |app, cx| app.apply_brain_settings(copy, restart, cx));
        }
        let revision = self.brain.settings_revision;
        match config::save_settings(&directory, &settings) {
            Ok(done) => {
                let work = cx.background_executor().spawn(async move {
                    done.recv().unwrap_or_else(|_| Err("保存线程已关闭".into()))
                });
                cx.spawn(async move |this, cx| {
                    if let Err(error) = work.await {
                        let _ = this.update(cx, |app, cx| {
                            if app.brain.settings_revision == revision {
                                app.desktop_workspace.error =
                                    Some(format!("保存幕僚设置失败：{error}").into());
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }
            Err(error) => {
                self.desktop_workspace.error = Some(format!("保存幕僚设置失败：{error}").into())
            }
        }
    }

    /// 接收新的设置；`restart` 为真时丢弃正在运行的大脑，下一帧按新配置启动。
    pub(crate) fn apply_brain_settings(
        &mut self,
        settings: BrainSettings,
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        let silenced = !settings.speak || !settings.enabled;
        self.brain.settings_revision += 1;
        super::owner::shared(cx).configure(settings.clone(), restart);
        self.brain.settings = Some(settings);
        if silenced {
            self.gpt_live.cancel_brain_reports();
            self.cancel_agent_announcements();
        }
        cx.notify();
    }

    pub(crate) fn brain_overview(&self) -> &super::OverviewSnapshot {
        &self.brain.overview
    }

    /// 工作空间窗口：汇总各项目会话里的大脑状态，供概览渲染。变化时才通知重绘。
    pub(crate) fn refresh_brain_overview(&mut self, cx: &mut Context<Self>) {
        let mut snapshot = super::OverviewSnapshot::default();
        for ((workspace_id, area_id), session) in &self.desktop_workspace.sessions {
            snapshot.running |= session.read(cx).brain.is_running();
            if self.desktop_workspace.active_id.as_ref() != Some(workspace_id) {
                continue;
            }
            snapshot.approvals.extend(session.update(cx, |app, _| {
                let waiting: Vec<_> = app
                    .runtime
                    .mobile_card_snapshots()
                    .into_iter()
                    .filter(|card| card.state == "waiting_approval")
                    .map(|card| card.node_id)
                    .collect();
                waiting
                    .into_iter()
                    .flat_map(|node| {
                        app.runtime
                            .cached_mobile_approval_snapshots(node)
                            .unwrap_or_default()
                    })
                    .filter(|approval| !approval.decisions.is_empty())
                    .map(|approval| (area_id.clone(), approval))
                    .collect::<Vec<_>>()
            }));
            let app = session.read(cx);
            snapshot
                .questions
                .extend(app.question_overview_rows(area_id));
            snapshot.judging.extend(
                app.brain_judging_nodes()
                    .into_iter()
                    .map(|node_id| (area_id.clone(), node_id)),
            );
            snapshot
                .proposals
                .extend(
                    app.brain
                        .proposals
                        .items()
                        .iter()
                        .map(|p| super::OverviewProposal {
                            area_id: area_id.clone(),
                            new_card: app.proposal_needs_card(&p.agent),
                            block: app.proposal_dispatch_block(&p.agent),
                            proposal: p.clone(),
                        }),
                );
        }
        snapshot.proposals.sort_by(|a, b| {
            b.proposal
                .at
                .cmp(&a.proposal.at)
                .then_with(|| b.proposal.id.cmp(&a.proposal.id))
                .then_with(|| a.area_id.cmp(&b.area_id))
        });
        self.brain.proposal_feedback.retain(|(area, id), feedback| {
            // 切换工作空间只隐藏行；实际派发完成之前不能解除防重复状态。
            if matches!(feedback, super::proposals::ProposalFeedback::Dispatching) {
                return true;
            }
            let Some(item) = snapshot
                .proposals
                .iter()
                .find(|p| &p.area_id == area && p.proposal.id == *id)
            else {
                return false;
            };
            // 忙碌/待确认由实时状态展示，恢复后不能留下过期的同义错误。
            !item.block.as_ref().is_some_and(|block| {
                matches!(feedback, super::proposals::ProposalFeedback::Failed(message)
                    if *message == block.message(&item.proposal.agent))
            })
        });
        if snapshot != self.brain.overview {
            let approvals_changed = snapshot.approvals != self.brain.overview.approvals;
            self.brain.overview = snapshot;
            if approvals_changed {
                self.note_workspace_activity_changed(cx);
            }
            cx.notify();
        }
    }

    /// 用户点击后才安装：把对应后端的 ACP 适配器装进 `~/.norma/brain/adapters`。
    pub(crate) fn install_brain_adapter(&mut self, kind: BackendKind, cx: &mut Context<Self>) {
        let Some(package) = adapter_package(kind) else {
            return;
        };
        if self.brain.installing.is_some() {
            return;
        }
        self.brain.installing = Some(kind);
        self.brain.install_error = None;
        let work = cx.background_executor().spawn(async move {
            let dir = config::state_dir()
                .ok_or_else(|| "找不到用户目录".to_string())?
                .join("adapters");
            std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let path =
                std::env::join_paths(config::search_dirs()).map_err(|error| error.to_string())?;
            let output = std::process::Command::new("npm")
                .args(["install", "--no-audit", "--no-fund", "--prefix"])
                .arg(&dir)
                .arg(package)
                .env("PATH", path)
                .output()
                .map_err(|error| format!("无法运行 npm：{error}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("npm 安装失败")
                    .to_string())
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.brain.installing = None;
                app.brain.install_error = result.err().map(|reason| (kind, reason));
                app.refresh_brain_settings(cx);
            });
        })
        .detach();
        cx.notify();
    }
}
