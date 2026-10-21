//! 桌面工作空间想法清单的宿主侧状态：缓存、刷新、删除撤销与指派入口。
//! 读写 SQLite 都在后台执行器上完成；前台只替换缓存并通知重绘。

use super::ideas_rows::{ideas_screen, IdeasScreen};
use super::ideas_sync::{sync_ideas, DispatchLookup, STUCK_AFTER_SECS};
use crate::app::MultisoulApp;
use agent::IdeaTarget;
use gpui::{AppContext as _, Context, Window};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use storage::IdeasStore;
use ui::component::input::{InputEvent, InputState};
use workspace::ideas::{classify_task_state, TaskPhase};

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
    /// 已加载项目协调器里的任务快照：(task_ref, 任务正文, 阶段)，以及各项目派发库路径。
    fn idea_task_sources(&self, cx: &Context<Self>) -> DispatchLookup {
        let mut lookup = DispatchLookup {
            live: Vec::new(),
            project_dbs: Vec::new(),
        };
        let Some(workspace_id) = self.desktop_workspace.active_id.as_deref() else {
            return lookup;
        };
        for area in &self.desktop_workspace.areas {
            lookup
                .project_dbs
                .push(self.database_paths.project_db_path(&area.root));
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area.id.clone()))
            else {
                continue;
            };
            if let Some(coordinator) = session.read(cx).agent_task_coordinator.as_ref() {
                lookup
                    .live
                    .extend(coordinator.tasks().into_iter().map(|task| {
                        let mut phase = classify_task_state(task.state);
                        // 无法追踪完成的任务（“untracked”在 tracking 字段里，不在 state 里）。
                        if task.tracking == agent::AgentTrackingMode::Untracked
                            && matches!(phase, TaskPhase::Queued | TaskPhase::Running)
                        {
                            phase = TaskPhase::Untracked;
                        }
                        (task.task_ref, task.task, phase)
                    }));
            }
        }
        lookup
    }

    /// 后台重读当前工作空间的清单，并把任务终态回写进想法（见 `ideas_sync`）。
    /// 读取各项目会话必须在前台且不能与正在更新的实体冲突：整个刷新从下一轮开始，
    /// 因此无论从项目会话、宿主还是快捷键调用都安全。只保留最新一次，迟到的结果按工作空间校验后丢弃。
    pub(crate) fn refresh_ideas(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        self.desktop_workspace.ideas_refresh = Some(cx.spawn(async move |this, cx| {
            let Ok(lookup) = this.update(cx, |app, cx| app.idea_task_sources(cx)) else {
                return;
            };
            let id = workspace_id.clone();
            let list = cx
                .background_spawn(async move {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or_default();
                    sync_ideas(&store, &id, &lookup, now, STUCK_AFTER_SECS)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                match list {
                    Ok(list) => app.desktop_workspace.ideas = list,
                    Err(error) => tracing::warn!(%error, "failed to sync workspace ideas"),
                }
                cx.notify();
            });
        }));
    }

    /// 想法清单顶部的输入框：回车或点“添加”记一条手动想法。创建需要窗口，所以在渲染前惰性建立。
    pub(super) fn ensure_idea_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.ideas_input.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("写下一个想法，回车添加"));
        let subscription =
            cx.subscribe_in(&input, window, |app, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    app.add_idea_from_input(window, cx);
                }
            });
        self.desktop_workspace.ideas_input = Some(input);
        self.desktop_ideas_input_subscription = Some(subscription);
    }

    /// 把输入框里的文字记成一条手动想法：写库在后台，完成后刷新清单；超长或为空给出提示。
    pub(super) fn add_idea_from_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(input), Some(workspace_id)) = (
            self.desktop_workspace.ideas_input.clone(),
            self.desktop_workspace.active_id.clone(),
        ) else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let body = match workspace::ideas::normalize_body(&text) {
            Ok(body) => body,
            Err(workspace::ideas::IdeaError::Empty) => return,
            Err(_) => {
                self.desktop_workspace.ideas_notice = Some("想法太长了，最多 500 个字".to_owned());
                cx.notify();
                return;
            }
        };
        input.update(cx, |state, cx| state.set_value("", window, cx));
        self.desktop_workspace.ideas_notice = None;
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move {
                    store.capture(&workspace_id, &body, storage::IdeaSource::Manual)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if let Err(error) = saved {
                    app.desktop_workspace.ideas_notice = Some(format!("没能记下：{error}"));
                }
                app.refresh_ideas(cx);
            });
        })
        .detach();
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
        let sources = self.idea_task_sources(cx);
        let phase_of = |task_ref: &str| {
            sources
                .live
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

#[cfg(test)]
#[path = "../../../tests/unit/desktop_ideas_flow_tests.rs"]
mod flow_tests;
