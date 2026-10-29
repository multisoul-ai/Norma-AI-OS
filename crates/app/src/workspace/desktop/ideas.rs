//! 桌面工作空间想法清单的宿主侧状态：缓存、刷新、删除撤销与指派入口。
//! 读写 SQLite 都在后台执行器上完成；前台只替换缓存并通知重绘。

mod idea_assign;
mod idea_badges;
mod idea_compose;
mod idea_picker;
pub(in crate::workspace::desktop) mod idea_reorder;

use super::ideas_rows::{ideas_screen, IdeasScreen};
use super::ideas_sync::{sync_ideas, DispatchLookup, STUCK_AFTER_SECS};
use super::ideas_undo::IdeaUndo;
use crate::app::MultisoulApp;
use agent::IdeaTarget;
use gpui::{AppContext as _, Context, Window};
use norma::protocol::AgentProfile;
use std::time::{SystemTime, UNIX_EPOCH};
use storage::IdeasStore;
use workspace::ideas::{classify_task_state, TaskPhase};

/// 想法栏的辅助状态：新建智能体的默认类型（拖到新建区、菜单新建、`⌃0` 共用），
/// 以及上一帧绘制出的待办行矩形（窗口坐标、按显示顺序），拖动排序时用来定位插入点。
pub(crate) struct IdeasAux {
    pub(crate) default_profile: AgentProfile,
    /// 上一次修饰键里的（⌃，⇧）；只在变化时重绘序号提示。
    pub(crate) badge_keys: (bool, bool),
    /// `⌃⇧0` 类型选择器：打开时是高亮行的下标。
    pub(crate) picker: Option<usize>,
    /// 默认类型已从设置库读回（或已被本次运行里的选择覆盖）；之后不再用库里的值回写界面。
    pub(crate) profile_loaded: bool,
    /// 已粘贴、还没随想法保存的图片文件名（按粘贴顺序）。
    pub(crate) pending_images: Vec<String>,
    /// 输入框是否拥有键盘焦点：用来绘制 Material 式的焦点描边。
    pub(crate) input_focused: bool,
    /// 新增待办后还要贴底滚动的帧数：输入框回落一行会让列表在下一帧变高，只滚一帧会差一行。
    pub(crate) stick_bottom_frames: u8,
    pub(crate) row_bounds:
        std::sync::Arc<std::sync::Mutex<Vec<(String, gpui::Bounds<gpui::Pixels>)>>>,
}

impl Default for IdeasAux {
    fn default() -> Self {
        Self {
            default_profile: AgentProfile::Codex,
            badge_keys: (false, false),
            picker: None,
            profile_loaded: false,
            pending_images: Vec::new(),
            input_focused: false,
            stick_bottom_frames: 0,
            row_bounds: Default::default(),
        }
    }
}

/// 菜单里的一个已有智能体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IdeaTargetOption {
    pub(super) area_id: String,
    pub(super) area_name: String,
    pub(super) node_id: u64,
    pub(super) name: String,
    /// 菜单右侧的状态文字与圆点颜色：运行中 / 等待审批 / 待开始。
    pub(super) status: &'static str,
    pub(super) running: bool,
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
        profile: AgentProfile,
    },
}

impl MultisoulApp {
    /// 已加载项目协调器里的任务快照：(task_ref, 任务正文, 阶段)，以及各项目派发库路径。
    fn idea_task_sources(&self, cx: &Context<Self>) -> DispatchLookup {
        let mut lookup = DispatchLookup {
            live: Vec::new(),
            project_dbs: Vec::new(),
            unavailable_projects: false,
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
        let db_path = self.database_paths.app_db_path().to_owned();
        let store = IdeasStore::new(db_path.clone());
        self.desktop_workspace.ideas_refresh = Some(cx.spawn(async move |this, cx| {
            let Ok((mut lookup, paths, roots)) = this.update(cx, |app, cx| {
                (
                    app.idea_task_sources(cx),
                    app.database_paths.clone(),
                    app.desktop_workspace
                        .areas
                        .iter()
                        .map(|area| area.root.clone())
                        .collect::<Vec<_>>(),
                )
            }) else {
                return;
            };
            let id = workspace_id.clone();
            let (list, saved_profile) = cx
                .background_spawn(async move {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or_default();
                    lookup.prepare_projects(&paths, &roots);
                    let list = sync_ideas(&store, &id, &lookup, now, STUCK_AFTER_SECS);
                    let saved = storage::SettingsStore::new(db_path)
                        .get_string(crate::shortcuts::idea_assign::DEFAULT_PROFILE_SETTING)
                        .ok()
                        .flatten();
                    (list, saved)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                let aux = &mut app.desktop_workspace.ideas_aux;
                if !aux.profile_loaded {
                    aux.profile_loaded = true;
                    if let Some(profile) = saved_profile
                        .as_deref()
                        .and_then(crate::shortcuts::idea_assign::profile_from_key)
                    {
                        aux.default_profile = profile;
                    }
                }
                match list {
                    Ok(list) => app.desktop_workspace.ideas = list,
                    Err(error) => tracing::warn!(%error, "failed to sync workspace ideas"),
                }
                cx.notify();
            });
        }));
    }

    /// 改默认新建类型：界面立即生效，写库放后台；之后库里的旧值不再覆盖它。
    pub(super) fn set_default_idea_profile(
        &mut self,
        profile: AgentProfile,
        cx: &mut Context<Self>,
    ) {
        let aux = &mut self.desktop_workspace.ideas_aux;
        aux.default_profile = profile;
        aux.profile_loaded = true;
        let store = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        cx.background_spawn(async move {
            let key = crate::shortcuts::idea_assign::DEFAULT_PROFILE_SETTING;
            if let Err(error) =
                store.set_string(key, crate::shortcuts::idea_assign::profile_key(profile))
            {
                tracing::warn!(%error, "failed to persist default idea agent type");
            }
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
        // 清空「已完成」后、撤销窗口内：这些想法暂不显示（库里还在，撤销可恢复）。
        let visible: Vec<_> = self
            .desktop_workspace
            .ideas
            .iter()
            .filter(|idea| {
                !self
                    .desktop_workspace
                    .ideas_pending_purge
                    .contains(&idea.id)
            })
            .cloned()
            .collect();
        ideas_screen(&visible, now, &names, &phase_of)
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
                    let (status, running) = match project.runtime.agent_task_state(node.id) {
                        Some(
                            runtime::AgentTaskState::Thinking
                            | runtime::AgentTaskState::RunningTool { .. },
                        ) => (ui::i18n::text("drawers.running"), true),
                        Some(runtime::AgentTaskState::WaitingQuestion) => {
                            (ui::i18n::text("drawers.waiting_answer"), false)
                        }
                        Some(runtime::AgentTaskState::WaitingApproval { .. }) => {
                            (ui::i18n::text("drawers.waiting_approval"), false)
                        }
                        _ => (ui::i18n::text("drawers.not_started"), false),
                    };
                    options.push(IdeaTargetOption {
                        area_id: area.id.clone(),
                        area_name: area.display_name.clone(),
                        node_id: node.id,
                        name,
                        status,
                        running,
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
        self.push_idea_undo(IdeaUndo::Delete {
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
        self.schedule_idea_undo_expiry(cx);
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
            IdeaUiTarget::New { area_id, profile } => (
                area_id.clone(),
                IdeaTarget::NewAgent {
                    workspace_id: workspace_id.clone(),
                    area_id,
                    profile,
                },
            ),
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id))
            .cloned()
        else {
            self.desktop_workspace.ideas_notice =
                Some(ui::i18n::LocalizedText::new("drawers.project_not_loaded"));
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
                    app.desktop_workspace.ideas_notice = Some(ui::i18n::message(
                        "drawers.assign_failed",
                        &[("error", (error).to_string())],
                    ));
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
