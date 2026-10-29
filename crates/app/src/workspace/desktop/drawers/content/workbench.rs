//! Project content state; directory IO and Git commands live in workspace::content.
use gpui::{Entity, Task};
use std::collections::{BTreeSet, HashMap};
use ui::component::input::InputState;
use workspace::content::{Comparison, FileOptions, GitSnapshot, Project, TextFile};

mod controller;
mod diff;
mod diff_view;
mod document;
mod editor;
mod layout;
mod leaving;
mod navigation;
pub(crate) mod open_path;
mod selectors;
mod sidebar;
mod sidebar_ops;
mod text_index;
mod tree;
mod tree_row;
mod view;
use crate::preview::content_copy::{change_status, content_error};
use crate::preview::local_content as local_preview;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct SavedWorkbench {
    pub area_id: Option<String>,
    pub selected: Option<String>,
    pub expanded: BTreeSet<String>,
    pub hidden: bool,
    pub ignored: bool,
    pub comparison: Comparison,
}

pub(crate) struct FileRow {
    pub path: String,
    pub label: String,
    pub directory: bool,
    pub depth: usize,
}

pub(crate) struct Draft {
    pub input: Entity<InputState>,
    pub original: TextFile,
    pub project: Project,
    pub path: String,
    pub saving: bool,
    pub dirty: bool,
}

#[derive(Default)]
pub(crate) struct WorkbenchState {
    pub saved: SavedWorkbench,
    pub epoch: WorkbenchEpoch,
    pub project_area: Option<String>,
    pub project_picker: Option<Entity<ChoiceState>>,
    pub branch_picker: Option<Entity<ChoiceState>>,
    pub project_choices: Vec<(String, String)>,
    pub branch_choices: Vec<String>,
    pub project: Option<Project>,
    pub followed_area: Option<String>,
    pub rows: Vec<FileRow>,
    pub git: Option<GitSnapshot>,
    pub git_error: Option<ui::i18n::LocalizedText>,
    pub file: Option<TextFile>,
    pub preview: Option<local_preview::LocalPreview>,
    pub drafts: HashMap<String, Draft>,
    pub diff: Option<String>,
    pub diff_parsed: Option<std::sync::Arc<Vec<diff::DiffLine>>>,
    pub text_index: std::cell::RefCell<Option<text_index::TextIndex>>,
    pub show_diff: bool,
    pub diff_version: Option<String>,
    pub diff_comparison: Comparison,
    pub diff_staged: bool,
    pub show_source: bool,
    pub sidebar_open: bool,
    pub changes_section_closed: bool,
    pub files_section_closed: bool,
    pub changes_dirs_closed: BTreeSet<String>,
    pub ops_open: bool,
    pub commit_open: bool,
    pub generation: u64,
    pub file_generation: u64,
    pub refreshing: bool,
    pub refresh_requested: bool,
    pub persisted_json: Option<String>,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<ui::i18n::LocalizedText>,
    pub notice: Option<ui::i18n::LocalizedText>,
    pub filter: Option<Entity<InputState>>,
    pub commit: Option<Entity<InputState>>,
    pub timer: Option<Task<()>>,
    pub persistence: Option<Task<()>>,
    pub confirm_discard: Option<Vec<String>>,
    pub pending_close: Option<PendingWorkbenchAction>,
    /// 终端点击路径后等待项目加载完成再打开的文件。
    pub pending_open: Option<open_path::PendingOpen>,
    /// 待滚动并高亮的 0 起始行号；`pending_scroll` 在文件加载完成时消费一次。
    pub jump_line: Option<usize>,
    pub pending_scroll: Option<usize>,
    /// 成果行打开右栏时只显示这些路径；用户改选项目后清空。
    pub turn_paths: Option<Vec<String>>,
    /// 为真时不要用当前展开项目覆盖成果所属项目。
    pub lock_area: bool,
    /// Git 快照到达后要直接打开差异的单个路径。
    pub pending_turn_diff: Option<String>,
}

impl WorkbenchState {
    pub fn options(&self) -> FileOptions {
        FileOptions {
            hidden: self.saved.hidden,
            ignored: self.saved.ignored,
        }
    }

    pub fn draft_key(&self) -> Option<String> {
        Some(
            self.project
                .as_ref()?
                .root()
                .join(self.saved.selected.as_ref()?)
                .to_string_lossy()
                .to_string(),
        )
    }

    pub fn has_edits(&self) -> bool {
        self.drafts.values().any(|d| d.dirty || d.saving) || self.busy
    }
}

#[derive(Clone)]
pub(crate) struct Choice {
    pub id: String,
    pub title: String,
}
impl ui::component::select::SelectItem for Choice {
    type Value = String;
    fn title(&self) -> gpui::SharedString {
        self.title.clone().into()
    }
    fn value(&self) -> &String {
        &self.id
    }
}
pub(crate) type ChoiceState =
    ui::component::select::SelectState<ui::component::select::SearchableVec<Choice>>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkbenchEpoch(u64);
impl Default for WorkbenchEpoch {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub(crate) enum PendingWorkbenchAction {
    CloseTab(String),
    CloseWindow(gpui::AnyWindowHandle),
    SwitchWorkspace(String, gpui::AnyWindowHandle),
    Quit,
}

#[cfg(test)]
#[path = "../../../../../tests/unit/content_workbench_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../../tests/unit/content_explorer_model_tests.rs"]
mod explorer_model_tests;

#[cfg(test)]
#[path = "../../../../../tests/unit/content_explorer_surface_tests.rs"]
mod explorer_surface_tests;

#[cfg(test)]
#[path = "../../../../../tests/unit/content_explorer_text_tests.rs"]
mod explorer_text_tests;
