use super::*;
use crate::app::MultisoulApp;
use gpui::{AppContext, Context, Window};
use std::{path::Path, time::Duration};
use storage::DesktopContentKind;
use ui::component::input::InputEvent;
use workspace::content::GitAction;

impl MultisoulApp {
    pub(crate) fn open_file_workbench(&mut self, cx: &mut Context<Self>) {
        self.open_desktop_content_tab(
            DesktopContentKind::ProjectFiles,
            "project-files".into(),
            "文件".into(),
            None,
            cx,
        );
    }

    /// Opens the file workbench with the "已更改的文件" section expanded.
    pub(crate) fn open_changes_view(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.workbench.changes_section_closed = false;
        self.open_file_workbench(cx);
    }

    pub(crate) fn ensure_file_workbench(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.workbench.filter.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("筛选文件…"));
            cx.subscribe(&input, |app, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    app.refresh_file_workbench(cx);
                }
            })
            .detach();
            self.desktop_workspace.workbench.filter = Some(input);
            self.desktop_workspace.workbench.commit =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("提交说明")));
        }
        let current = self
            .desktop_workspace
            .expanded_area_id
            .clone()
            .or(self.desktop_workspace.active_area_id.clone());
        let state = &mut self.desktop_workspace.workbench;
        let follow = state.followed_area != current && current.is_some();
        state.followed_area = current.clone();
        if follow && state.saved.area_id != current {
            state.saved.area_id = current;
            state.saved.selected = None;
            state.saved.expanded.clear();
        }
        if state.saved.area_id.is_none() {
            state.saved.area_id = self.desktop_workspace.areas.first().map(|a| a.id.clone());
        }
        if state.project_area != state.saved.area_id {
            state.project_area = state.saved.area_id.clone();
            state.generation = state.generation.wrapping_add(1);
            state.file_generation = state.file_generation.wrapping_add(1);
            state.rows.clear();
            state.git = None;
            state.project = None;
            state.file = None;
            state.preview = None;
            state.diff = None;
            self.refresh_file_workbench(cx);
        }
        self.ensure_workbench_selectors(window, cx);
        if self.desktop_workspace.workbench.timer.is_none() {
            self.desktop_workspace.workbench.timer = Some(cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |app, cx| {
                        let visible =
                            app.desktop_workspace.drawers.right_open
                                && app.desktop_workspace.content_tabs.iter().any(|t| {
                                    t.selected && t.kind == DesktopContentKind::ProjectFiles
                                });
                        if visible
                            && !app.desktop_workspace.workbench.busy
                            && !app.desktop_workspace.workbench.refreshing
                        {
                            app.refresh_file_workbench(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }));
        }
    }

    pub(crate) fn refresh_file_workbench(&mut self, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        if s.refreshing {
            s.refresh_requested = true;
            return;
        }
        let Some(root) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|a| Some(&a.id) == s.saved.area_id.as_ref())
            .map(|a| a.root.clone())
        else {
            s.project = None;
            s.error = Some("请先添加或选择项目".into());
            return;
        };
        s.generation = s.generation.wrapping_add(1);
        let generation = s.generation;
        let epoch = s.epoch;
        let area = s.saved.area_id.clone();
        let expanded = s.saved.expanded.clone();
        let options = s.options();
        let comparison = s.saved.comparison.clone();
        let query = s
            .filter
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let selected = s.saved.selected.clone();
        s.refreshing = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let project = Project::open(&root)?;
                    let mut rows = Vec::new();
                    if query.trim().is_empty() {
                        append_directory(
                            &project,
                            Path::new(""),
                            0,
                            &expanded,
                            options,
                            &mut rows,
                        )?;
                    } else {
                        let hits = project
                            .search(&query, options)?
                            .into_iter()
                            .map(|e| (e.path.to_string_lossy().to_string(), e.directory))
                            .collect::<Vec<_>>();
                        rows = super::tree::search_rows(&hits);
                    }
                    let mut git = project.git_snapshot(&Comparison::Local);
                    let mut git_error = None;
                    if let Comparison::Branch(target) = &comparison {
                        match project.git_snapshot(&comparison) {
                            Ok(snapshot) => git = Ok(snapshot),
                            Err(error) => {
                                if let Ok(snapshot) = &mut git {
                                    snapshot.changes.clear();
                                    snapshot.writable = false;
                                }
                                git_error = Some(if target.is_empty() {
                                    "请选择比较分支".into()
                                } else {
                                    error
                                });
                            }
                        }
                    }
                    let disk_version = selected
                        .as_ref()
                        .and_then(|p| project.read_text(Path::new(p)).ok())
                        .map(|f| f.version);
                    Ok::<_, String>((project, rows, git, git_error, disk_version))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                let s = &mut app.desktop_workspace.workbench;
                if s.epoch != epoch {
                    return;
                }
                s.refreshing = false;
                if generation != s.generation || area != s.saved.area_id || s.refresh_requested {
                    s.refresh_requested = false;
                    app.refresh_file_workbench(cx);
                    return;
                }
                match result {
                    Ok((project, rows, git, git_error, disk_version)) => {
                        let changed_project =
                            s.project.as_ref().map(Project::root) != Some(project.root());
                        s.project = Some(project);
                        s.rows = rows;
                        match git {
                            Ok(git) => {
                                s.git = Some(git);
                                s.git_error = git_error;
                            }
                            Err(e) => {
                                s.git = None;
                                s.git_error = Some(e);
                            }
                        }
                        let reload_diff = s.show_diff
                            && (s.diff_comparison != s.saved.comparison
                                || s.diff_version != s.git.as_ref().map(|g| g.version.clone()));
                        let dirty = s
                            .draft_key()
                            .as_ref()
                            .and_then(|k| s.drafts.get(k))
                            .is_some_and(|d| d.dirty || d.saving);
                        let external_change = s.file.as_ref().is_some_and(|f| {
                            disk_version.as_ref().is_some_and(|v| v != &f.version)
                        });
                        if external_change && dirty {
                            s.notice = Some("文件已被外部修改；保存前请合并，草稿仍保留".into());
                        }
                        if reload_diff {
                            let staged = s.diff_staged;
                            app.load_workbench_file(true, staged, cx);
                        } else if (changed_project || (external_change && !dirty))
                            && s.saved.selected.is_some()
                        {
                            if !dirty {
                                if let Some(key) = s.draft_key() {
                                    s.drafts.remove(&key);
                                }
                            }
                            app.load_workbench_file(false, false, cx);
                        }
                    }
                    Err(error) => {
                        s.error = Some(error);
                        s.pending_open = None;
                    }
                }
                app.apply_pending_workbench_open(cx);
                app.persist_file_workbench(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn persist_file_workbench(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Ok(json) = serde_json::to_string(&self.desktop_workspace.workbench.saved) else {
            return;
        };
        if self.desktop_workspace.workbench.persisted_json.as_ref() == Some(&json) {
            return;
        }
        self.desktop_workspace.workbench.persisted_json = Some(json.clone());
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let previous = self.desktop_workspace.workbench.persistence.take();
        self.desktop_workspace.workbench.persistence = Some(cx.spawn(async move |_, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let _ = cx
                .background_spawn(async move { store.save_state(&workspace, &json) })
                .await;
        }));
    }

    pub(crate) fn select_workbench_path(
        &mut self,
        path: String,
        diff: bool,
        staged: bool,
        cx: &mut Context<Self>,
    ) {
        self.hide_desktop_content_browsers();
        let s = &mut self.desktop_workspace.workbench;
        s.saved.selected = Some(path);
        s.jump_line = None;
        s.pending_scroll = None;
        s.sidebar_open = false;
        s.show_source = false;
        self.load_workbench_file(diff, staged, cx);
        self.persist_file_workbench(cx);
    }

    pub(crate) fn load_workbench_file(&mut self, diff: bool, staged: bool, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        let (Some(project), Some(path)) = (s.project.clone(), s.saved.selected.clone()) else {
            return;
        };
        let comparison = s.saved.comparison.clone();
        s.file_generation = s.file_generation.wrapping_add(1);
        let generation = s.file_generation;
        let epoch = s.epoch;
        let selected = s.saved.selected.clone();
        let project_root = project.root().to_path_buf();
        s.file = None;
        s.preview = None;
        s.diff = None;
        s.error = None;
        s.loading = true;
        s.diff_staged = staged;
        s.show_diff = diff;
        s.diff_version = s.git.as_ref().map(|g| g.version.clone());
        s.diff_comparison = s.saved.comparison.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if diff {
                        return project
                            .git_diff(&comparison, &path, staged)
                            .map(|d| (Some(d), None, None));
                    }
                    let kind = local_preview::kind(&path);
                    let text = if matches!(
                        kind,
                        local_preview::Kind::Text
                            | local_preview::Kind::Markdown
                            | local_preview::Kind::Html
                    ) {
                        Some(project.read_text(Path::new(&path))?)
                    } else {
                        None
                    };
                    let preview = if kind != local_preview::Kind::Text {
                        Some(local_preview::LocalPreview::new(
                            project.root(),
                            &path,
                            text.as_ref().map(|t| t.text.as_str()),
                        )?)
                    } else {
                        None
                    };
                    Ok::<_, String>((None, text, preview))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                let s = &mut app.desktop_workspace.workbench;
                if s.epoch != epoch
                    || s.file_generation != generation
                    || s.saved.selected != selected
                    || s.project.as_ref().map(Project::root) != Some(project_root.as_path())
                {
                    return;
                }
                s.loading = false;
                match result {
                    Ok((diff, text, preview)) => {
                        s.diff_parsed = diff
                            .as_deref()
                            .map(|p| std::sync::Arc::new(super::diff::parse_diff_for_display(p)));
                        s.diff = diff;
                        s.file = text;
                        s.preview = preview;
                    }
                    Err(e) => s.error = Some(e),
                }
                app.consume_workbench_scroll();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn workbench_git_action(&mut self, action: GitAction, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        if s.busy || s.saved.comparison != Comparison::Local {
            return;
        }
        let (Some(project), Some(snapshot)) = (s.project.clone(), s.git.as_ref()) else {
            return;
        };
        if self
            .desktop_workspace
            .areas
            .iter()
            .find(|a| Some(&a.id) == s.saved.area_id.as_ref())
            .is_none_or(|a| a.root != project.root())
        {
            return;
        }
        let version = snapshot.version.clone();
        let generation = s.generation;
        let epoch = s.epoch;
        s.busy = true;
        s.error = None;
        s.notice = Some("正在执行 Git 操作…".into());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { project.git_action(&version, action) })
                .await;
            let _ = this.update(cx, |app, cx| {
                let s = &mut app.desktop_workspace.workbench;
                if s.epoch != epoch {
                    return;
                }
                s.busy = false;
                if generation != s.generation {
                    app.finish_workbench_leaving(cx);
                    cx.notify();
                    return;
                }
                match result {
                    Ok(_) => s.notice = Some("Git 操作完成".into()),
                    Err(e) => {
                        s.notice = None;
                        s.error = Some(e);
                    }
                }
                app.refresh_file_workbench(cx);
                app.finish_workbench_leaving(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn append_directory(
    project: &Project,
    path: &Path,
    depth: usize,
    expanded: &BTreeSet<String>,
    options: FileOptions,
    rows: &mut Vec<FileRow>,
) -> Result<(), String> {
    if depth > 64 {
        return Err("目录层级超过 64 层".into());
    }
    for e in project.directory(path, options)? {
        let path = e.path.to_string_lossy().to_string();
        let open = e.directory && expanded.contains(&path);
        rows.push(FileRow {
            path: path.clone(),
            label: e.name,
            directory: e.directory,
            depth,
        });
        if open {
            append_directory(project, &e.path, depth + 1, expanded, options, rows)?;
        }
    }
    Ok(())
}
