//! 桌面工作空间想法清单的宿主侧状态：缓存、刷新、删除撤销与指派入口。
//! 读写 SQLite 都在后台执行器上完成；前台只替换缓存并通知重绘。

use super::ideas_rows::{ideas_screen, IdeasScreen};
use crate::app::MultisoulApp;
use agent::IdeaTarget;
use gpui::{AppContext as _, Context, Window};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use storage::IdeasStore;
use workspace::ideas::{classify_task_state, TaskPhase};

/// 停留超过这么久的“派发中”才会被对账，避免抢先回退仍在发送的想法。
const STUCK_AFTER_SECS: u64 = 120;
/// 已完成的想法保留的时长（7 天）。
const DONE_KEEP_SECS: u64 = 7 * 24 * 3600;
/// 删除后可撤销的时间窗口。
const UNDO_WINDOW: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdeaUndo {
    pub(crate) id: String,
    pub(crate) title: String,
}

/// 菜单里的一个已有智能体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IdeaTargetOption {
    pub(super) area_id: String,
    pub(super) area_name: String,
    pub(super) node_id: u64,
    pub(super) name: String,
}

/// 用户在界面里选的派发目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IdeaUiTarget {
    Existing {
        area_id: String,
        node_id: u64,
    },
    /// 在指定项目区域新建智能体来完成。
    New {
        area_id: String,
    },
}

impl MultisoulApp {
    /// 已加载项目协调器里的任务快照：(task_ref, 任务正文, 阶段)。
    fn idea_task_snapshot(&self, cx: &Context<Self>) -> Vec<(String, String, TaskPhase)> {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_deref() else {
            return Vec::new();
        };
        let mut tasks = Vec::new();
        for area in &self.desktop_workspace.areas {
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area.id.clone()))
            else {
                continue;
            };
            if let Some(coordinator) = session.read(cx).agent_task_coordinator.as_ref() {
                tasks.extend(
                    coordinator
                        .tasks()
                        .into_iter()
                        .map(|task| (task.task_ref, task.task, classify_task_state(task.state))),
                );
            }
        }
        tasks
    }

    /// 后台重读当前工作空间的清单，并把任务终态回写进想法：成功→完成、取消→回待办，
    /// 超过一段时间的“派发中”按真实任务对账，已完成保留 7 天后清理。
    /// 只保留最新一次，迟到的结果按工作空间校验后丢弃。
    pub(crate) fn refresh_ideas(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let tasks = self.idea_task_snapshot(cx);
        let id = workspace_id.clone();
        self.desktop_workspace.ideas_refresh = Some(cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move {
                    let phase_of = |task_ref: &str| {
                        tasks
                            .iter()
                            .find(|(candidate, _, _)| candidate == task_ref)
                            .map(|(_, _, phase)| *phase)
                    };
                    let resolve = |idea: &storage::StoredIdea| {
                        tasks
                            .iter()
                            .find(|(_, text, _)| *text == idea.body)
                            .map(|(task_ref, _, _)| task_ref.clone())
                    };
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or_default();
                    let reconciled =
                        store.reconcile_dispatching_with(&id, STUCK_AFTER_SECS, resolve);
                    let applied = store.apply_task_phases(&id, phase_of);
                    let purged = store.purge_done_before(&id, now.saturating_sub(DONE_KEEP_SECS));
                    for outcome in [reconciled, applied, purged] {
                        if let Err(error) = outcome {
                            tracing::warn!(%error, "idea housekeeping failed");
                        }
                    }
                    store.list(&id)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                match list {
                    Ok(list) => app.desktop_workspace.ideas = list,
                    Err(error) => tracing::warn!(%error, "failed to load workspace ideas"),
                }
                cx.notify();
            });
        }));
    }

    /// 失败的想法退回待办重试。
    pub(super) fn retry_idea(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let target = id.to_owned();
        cx.spawn(async move |this, cx| {
            let _ = cx
                .background_spawn(async move { store.retract(&workspace_id, &target) })
                .await;
            let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
        })
        .detach();
    }

    pub(super) fn ideas_screen(&self, cx: &Context<Self>) -> IdeasScreen {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or_default();
        let names = |area: &str, node: u64| self.idea_agent_name(area, node, cx);
        let tasks = self.idea_task_snapshot(cx);
        let phase_of = |task_ref: &str| {
            tasks
                .iter()
                .find(|(candidate, _, _)| candidate == task_ref)
                .map(|(_, _, phase)| *phase)
        };
        ideas_screen(&self.desktop_workspace.ideas, now, &names, &phase_of)
    }

    fn idea_agent_name(&self, area_id: &str, node_id: u64, cx: &Context<Self>) -> Option<String> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id.to_owned()))?;
        agent_label(session.read(cx), node_id)
    }

    /// 已加载项目里的全部智能体，按项目顺序。
    pub(super) fn idea_target_options(&self, cx: &Context<Self>) -> Vec<IdeaTargetOption> {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_deref() else {
            return Vec::new();
        };
        let mut options = Vec::new();
        for area in &self.desktop_workspace.areas {
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area.id.clone()))
            else {
                continue;
            };
            let project = session.read(cx);
            for node in project.runtime.document.nodes() {
                if !matches!(node.kind, canvas::CanvasNodeKind::Agent(_)) {
                    continue;
                }
                if let Some(name) = agent_label(project, node.id) {
                    options.push(IdeaTargetOption {
                        area_id: area.id.clone(),
                        area_name: area.display_name.clone(),
                        node_id: node.id,
                        name,
                    });
                }
            }
        }
        options
    }

    pub(super) fn delete_idea(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(position) = self
            .desktop_workspace
            .ideas
            .iter()
            .position(|idea| idea.id == id)
        else {
            return;
        };
        let removed = self.desktop_workspace.ideas.remove(position);
        let title = removed.body.lines().next().unwrap_or_default().to_owned();
        self.desktop_workspace.ideas_menu = None;
        self.desktop_workspace.ideas_undo = Some(IdeaUndo {
            id: removed.id.clone(),
            title,
        });
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let target = removed.id;
        cx.spawn(async move |this, cx| {
            let _ = cx
                .background_spawn(async move { store.dismiss(&workspace_id, &target) })
                .await;
            let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
        })
        .detach();
        self.desktop_workspace.ideas_undo_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNDO_WINDOW).await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.ideas_undo = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn undo_delete_idea(&mut self, cx: &mut Context<Self>) {
        let (Some(undo), Some(workspace_id)) = (
            self.desktop_workspace.ideas_undo.take(),
            self.desktop_workspace.active_id.clone(),
        ) else {
            return;
        };
        self.desktop_workspace.ideas_undo_timer = None;
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |this, cx| {
            let _ = cx
                .background_spawn(async move { store.restore(&workspace_id, &undo.id) })
                .await;
            let _ = this.update(cx, |app, cx| app.refresh_ideas(cx));
        })
        .detach();
        cx.notify();
    }

    /// 在目标项目的会话里派发：先让出宿主，避免会话回读宿主时重入借用。
    pub(super) fn dispatch_idea(
        &mut self,
        idea_id: String,
        target: IdeaUiTarget,
        from: Option<(f32, f32)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let (area_id, ideas_target) = match target {
            IdeaUiTarget::Existing { area_id, node_id } => (
                area_id.clone(),
                IdeaTarget::Agent {
                    workspace_id: workspace_id.clone(),
                    area_id,
                    node_id,
                },
            ),
            IdeaUiTarget::New { area_id } => (
                area_id.clone(),
                IdeaTarget::NewAgent {
                    workspace_id: workspace_id.clone(),
                    area_id,
                    profile: norma::protocol::AgentProfile::Codex,
                },
            ),
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id))
            .cloned()
        else {
            self.desktop_workspace.ideas_notice = Some("目标项目尚未加载".to_owned());
            cx.notify();
            return;
        };
        self.desktop_workspace.ideas_menu = None;
        self.desktop_workspace.ideas_notice = None;
        let host = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let result = session.update(cx, |app, cx| {
                crate::app::assign_idea_for_ui(app, window, cx, &idea_id, ideas_target, from)
            });
            let _ = host.update(cx, |app, cx| {
                if let Err(error) = result {
                    app.desktop_workspace.ideas_notice = Some(format!("派发失败：{error}"));
                }
                app.refresh_ideas(cx);
            });
        });
    }
}

fn agent_label(project: &MultisoulApp, node_id: u64) -> Option<String> {
    project.runtime.document.node(node_id).and_then(|node| {
        let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        Some(
            spec.alias
                .clone()
                .unwrap_or_else(|| spec.profile.display_title().to_string()),
        )
    })
}
