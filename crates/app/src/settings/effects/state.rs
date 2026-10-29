//! 特效设置的缓存与纯逻辑：谁生效、怎么改。渲染与派发只读这里的缓存，不碰 SQLite。

use crate::motion::run_style::RunStyle;
use crate::motion::StyleSelection;
use std::collections::HashMap;
use storage::{DispatchEffectPreference as Pref, DispatchEffectSnapshot};

/// 设置页网格的顺序；“random”不是具体款式，放在最后。
pub(crate) const GRID_STYLES: [&str; 8] = [
    "meteor",
    "plane",
    "ribbon",
    "firefly",
    "fireworks",
    "lightning",
    "vortex",
    "random",
];
const DEFAULT_STYLE: &str = "meteor";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EffectsTarget {
    Workspace {
        workspace_id: String,
    },
    Project {
        workspace_id: String,
        area_id: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    Voice,
    Idea,
    Handoff,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PlaybackSpeed {
    Relaxed,
    #[default]
    Standard,
    Brisk,
}

impl PlaybackSpeed {
    pub(crate) fn multiplier(self) -> f32 {
        match self {
            Self::Relaxed => 1.33,
            Self::Standard => 1.0,
            Self::Brisk => 0.75,
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Standard => "standard",
            Self::Brisk => "brisk",
        }
    }

    pub(crate) fn from_key(key: Option<&str>) -> Self {
        match key {
            Some("relaxed") => Self::Relaxed,
            Some("brisk") => Self::Brisk,
            _ => Self::Standard,
        }
    }
}

pub(crate) const KEY_SPEED: &str = "effects.speed";
/// 卡片运行特效风格：全局偏好，所有项目共用。
pub(crate) const KEY_RUN_STYLE: &str = "effects.run_style";

#[derive(Clone, Debug)]
pub(crate) struct EffectsSettingsState {
    workspaces: HashMap<String, Pref>,
    projects: HashMap<(String, String), Pref>,
    pub speed: PlaybackSpeed,
    /// 目前设置页是否在编辑项目（否则是工作空间）。
    pub edit_project: bool,
    pub status: Option<String>,
    pub busy: bool,
    /// 最近一次修改，供“撤销”恢复。
    pub undo: Option<(EffectsTarget, Option<Pref>)>,
    pub run_style: RunStyle,
    /// 运行特效分区自己的状态行与撤销值，和派发特效互不干扰。
    pub run_status: Option<String>,
    pub run_undo: Option<RunStyle>,
    /// 最近一次确认写入数据库的风格与其序号；`run_generation` 是最新一次保存的序号。
    pub run_saved: RunStyle,
    pub run_saved_generation: u64,
    pub run_generation: u64,
}

impl Default for EffectsSettingsState {
    fn default() -> Self {
        Self {
            workspaces: HashMap::new(),
            projects: HashMap::new(),
            speed: PlaybackSpeed::default(),
            edit_project: false,
            status: None,
            busy: false,
            undo: None,
            run_style: RunStyle::default(),
            run_status: None,
            run_undo: None,
            run_saved: RunStyle::default(),
            run_saved_generation: 0,
            run_generation: 0,
        }
    }
}

impl gpui::Global for EffectsSettingsState {}

impl EffectsSettingsState {
    pub(crate) fn load_snapshot(&mut self, snapshot: DispatchEffectSnapshot) {
        self.workspaces = snapshot.workspaces.into_iter().collect();
        self.projects = snapshot
            .projects
            .into_iter()
            .map(|(workspace, area, pref)| ((workspace, area), pref))
            .collect();
    }

    pub(crate) fn saved(&self, target: &EffectsTarget) -> Option<&Pref> {
        match target {
            EffectsTarget::Workspace { workspace_id } => self.workspaces.get(workspace_id),
            EffectsTarget::Project {
                workspace_id,
                area_id,
            } => self.projects.get(&(workspace_id.clone(), area_id.clone())),
        }
    }

    /// 项目覆盖 → 工作空间；都没有就是没有设置（用各来源的默认款式）。
    pub(crate) fn effective(&self, workspace: Option<&str>, area: Option<&str>) -> Option<&Pref> {
        let workspace = workspace?;
        area.and_then(|area| self.projects.get(&(workspace.to_owned(), area.to_owned())))
            .or_else(|| self.workspaces.get(workspace))
    }

    /// 目标当前生效的偏好（项目跟随时即工作空间的）。
    pub(crate) fn effective_for(&self, target: &EffectsTarget) -> Option<&Pref> {
        match target {
            EffectsTarget::Workspace { workspace_id } => self.effective(Some(workspace_id), None),
            EffectsTarget::Project {
                workspace_id,
                area_id,
            } => self.effective(Some(workspace_id), Some(area_id)),
        }
    }

    pub(crate) fn selection_for(
        &self,
        workspace: Option<&str>,
        area: Option<&str>,
    ) -> StyleSelection {
        self.effective(workspace, area)
            .map(StyleSelection::from_preference)
            .unwrap_or_default()
    }

    pub(crate) fn apply(&mut self, target: &EffectsTarget, pref: Option<Pref>) {
        match (target, pref) {
            (EffectsTarget::Workspace { workspace_id }, Some(pref)) => {
                self.workspaces.insert(workspace_id.clone(), pref);
            }
            (EffectsTarget::Workspace { workspace_id }, None) => {
                self.workspaces.remove(workspace_id);
            }
            (
                EffectsTarget::Project {
                    workspace_id,
                    area_id,
                },
                Some(pref),
            ) => {
                self.projects
                    .insert((workspace_id.clone(), area_id.clone()), pref);
            }
            (
                EffectsTarget::Project {
                    workspace_id,
                    area_id,
                },
                None,
            ) => {
                self.projects
                    .remove(&(workspace_id.clone(), area_id.clone()));
            }
        }
    }

    /// 网格里勾选的款式 id：项目跟随工作空间时显示工作空间的款式。
    pub(crate) fn selected_style(&self, target: &EffectsTarget) -> &str {
        let (workspace, area) = match target {
            EffectsTarget::Workspace { workspace_id } => (workspace_id.as_str(), None),
            EffectsTarget::Project {
                workspace_id,
                area_id,
            } => (workspace_id.as_str(), Some(area_id.as_str())),
        };
        self.effective(Some(workspace), area)
            .map(|pref| pref.style.as_str())
            .filter(|style| GRID_STYLES.contains(style))
            .unwrap_or(DEFAULT_STYLE)
    }

    pub(crate) fn follows_workspace(&self, target: &EffectsTarget) -> bool {
        matches!(target, EffectsTarget::Project { .. }) && self.saved(target).is_none()
    }

    fn base_for(&self, target: &EffectsTarget) -> Pref {
        self.saved(target)
            .cloned()
            .or_else(|| match target {
                EffectsTarget::Project { workspace_id, .. } => {
                    self.workspaces.get(workspace_id).cloned()
                }
                EffectsTarget::Workspace { .. } => None,
            })
            .unwrap_or_else(|| Pref {
                style: DEFAULT_STYLE.into(),
                voice: None,
                idea: None,
                handoff: None,
            })
    }

    /// 选了一个款式：保留已有的来源覆盖。
    pub(crate) fn picked(&self, target: &EffectsTarget, style: &str) -> Pref {
        Pref {
            style: style.into(),
            ..self.base_for(target)
        }
    }

    pub(crate) fn with_source(
        &self,
        target: &EffectsTarget,
        source: Source,
        value: Option<&str>,
    ) -> Pref {
        let mut pref = self.base_for(target);
        let slot = match source {
            Source::Voice => &mut pref.voice,
            Source::Idea => &mut pref.idea,
            Source::Handoff => &mut pref.handoff,
        };
        *slot = value.map(str::to_owned);
        pref
    }

    /// 关闭“跟随工作空间”时项目的起点：复制工作空间当前的设置。
    pub(crate) fn unfollowed(&self, target: &EffectsTarget) -> Pref {
        self.base_for(target)
    }
}
