//! 终端点击路径 → 文件工作台：等待项目加载、展开祖先目录、行号定位。

use crate::app::MultisoulApp;
use gpui::{Context, ScrollStrategy};
use std::path::PathBuf;

/// 点击后等待工作台项目就绪再打开的文件；`line` 为终端给出的 1 起始行号。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingOpen {
    pub area_id: String,
    pub rel: String,
    pub line: Option<usize>,
    /// 目录只在文件树中展开定位，不加载为文件。
    pub is_dir: bool,
}

impl PendingOpen {
    /// 工作台已离开目标项目：用户在加载期间切走，之后不应再被旧点击带回。
    pub(crate) fn is_stale(&self, saved_area: Option<&str>) -> bool {
        saved_area != Some(self.area_id.as_str())
    }

    /// 只有工作台已切到目标项目、且该项目的目录树已加载完成时才能选中文件。
    pub(crate) fn ready(
        &self,
        saved_area: Option<&str>,
        project_area: Option<&str>,
        loaded: bool,
    ) -> bool {
        loaded
            && saved_area == Some(self.area_id.as_str())
            && project_area == Some(self.area_id.as_str())
    }
}

/// `a/b/c.rs` → `["a", "a/b"]`：展开目录树时需要打开的祖先目录。
pub(crate) fn expand_ancestors(rel: &str) -> Vec<String> {
    let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    parts.pop();
    (1..=parts.len()).map(|n| parts[..n].join("/")).collect()
}

/// 文件展开祖先目录；目录还要展开自身，使其子项可见。
pub(crate) fn expand_chain(rel: &str, is_dir: bool) -> Vec<String> {
    let mut chain = expand_ancestors(rel);
    let own = rel
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if is_dir && !own.is_empty() {
        chain.push(own);
    }
    chain
}

/// 终端的 1 起始行号 → 文本里的 0 起始行索引，夹在 `[0, 行数-1]`。
pub(crate) fn jump_index(line: usize, text: &str) -> usize {
    let last = text.bytes().filter(|b| *b == b'\n').count();
    line.saturating_sub(1).min(last)
}

impl MultisoulApp {
    /// 会话实体侧：读取宿主的项目根与来源项目，供后台判断路径归属。
    pub(crate) fn desktop_area_context(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(Vec<(String, PathBuf)>, Option<String>)> {
        let host = self.desktop_host.as_ref().and_then(|h| h.upgrade())?;
        let source = cx.entity_id();
        Some(host.read_with(cx, |host, _| {
            let ws = &host.desktop_workspace;
            let areas = ws
                .areas
                .iter()
                .map(|a| (a.id.clone(), a.root.clone()))
                .collect();
            let preferred = ws
                .sessions
                .iter()
                .find_map(|((workspace_id, area_id), session)| {
                    (ws.active_id.as_deref() == Some(workspace_id.as_str())
                        && session.entity_id() == source)
                        .then(|| area_id.clone())
                });
            (areas, preferred)
        }))
    }

    /// 会话实体侧：把文件交给宿主的文件工作台；宿主缺失或项目已切换时返回 false。
    pub(crate) fn open_workbench_file_in_host(
        &mut self,
        area_id: String,
        rel: String,
        line: Option<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.desktop_host.as_ref().and_then(|h| h.upgrade()) else {
            return false;
        };
        host.update(cx, |host, cx| {
            host.open_file_in_workbench(area_id, rel, line, cx)
        })
    }

    /// 会话实体侧：让宿主的文件树展开并定位目录。
    pub(crate) fn reveal_workbench_dir_in_host(
        &mut self,
        area_id: String,
        rel: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.desktop_host.as_ref().and_then(|h| h.upgrade()) else {
            return false;
        };
        host.update(cx, |host, cx| {
            host.reveal_dir_in_workbench(area_id, rel, cx)
        })
    }

    pub(crate) fn open_file_in_workbench(
        &mut self,
        area_id: String,
        rel: String,
        line: Option<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.queue_workbench_open(area_id, rel, line, false, cx)
    }

    pub(crate) fn reveal_dir_in_workbench(
        &mut self,
        area_id: String,
        rel: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.queue_workbench_open(area_id, rel, None, true, cx)
    }

    fn queue_workbench_open(
        &mut self,
        area_id: String,
        rel: String,
        line: Option<usize>,
        is_dir: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.desktop_workspace.areas.iter().any(|a| a.id == area_id) {
            return false;
        }
        let current = self
            .desktop_workspace
            .expanded_area_id
            .clone()
            .or(self.desktop_workspace.active_area_id.clone());
        let s = &mut self.desktop_workspace.workbench;
        s.followed_area = current;
        if s.saved.area_id.as_deref() != Some(area_id.as_str()) {
            s.saved.area_id = Some(area_id.clone());
            s.saved.selected = None;
            s.saved.expanded.clear();
        }
        s.saved.expanded.extend(expand_chain(&rel, is_dir));
        s.pending_open = Some(PendingOpen {
            area_id,
            rel,
            line,
            is_dir,
        });
        self.open_file_workbench(cx);
        if self.apply_pending_workbench_open(cx) {
            self.refresh_file_workbench(cx);
        }
        cx.notify();
        true
    }

    /// 项目加载完成后调用；就绪则选中文件并登记待滚动行。
    pub(crate) fn apply_pending_workbench_open(&mut self, cx: &mut Context<Self>) -> bool {
        let s = &mut self.desktop_workspace.workbench;
        let Some(pending) = s.pending_open.clone() else {
            return false;
        };
        if pending.is_stale(s.saved.area_id.as_deref()) {
            s.pending_open = None;
            return false;
        }
        if !pending.ready(
            s.saved.area_id.as_deref(),
            s.project_area.as_deref(),
            s.project.is_some(),
        ) {
            return false;
        }
        s.pending_open = None;
        if pending.is_dir {
            s.saved.selected = None;
            s.sidebar_open = true;
            return true;
        }
        self.select_workbench_path(pending.rel, false, false, cx);
        let s = &mut self.desktop_workspace.workbench;
        let target = pending.line.map(|l| l.saturating_sub(1));
        s.pending_scroll = target;
        s.jump_line = target;
        true
    }

    /// 文件文本就绪后消费一次待滚动行，并修正为文本内有效行。
    pub(crate) fn consume_workbench_scroll(&mut self) {
        let s = &mut self.desktop_workspace.workbench;
        let (Some(line), Some(file)) = (s.pending_scroll.take(), s.file.as_ref()) else {
            return;
        };
        let index = jump_index(line + 1, &file.text);
        s.jump_line = Some(index);
        self.workbench_text_scroll
            .scroll_to_item_strict(index, ScrollStrategy::Center);
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/unit/workbench_open_path_tests.rs"]
mod tests;
