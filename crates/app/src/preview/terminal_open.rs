use super::terminal_path::{resolve_terminal_path, TerminalPathHit};
use super::terminal_route::{
    inspect_path, locate_in_areas, route_terminal_path, TerminalPathRoute,
};
#[cfg(target_os = "macos")]
use super::terminal_route::{probe_system_open, SystemOpen};
use crate::app::MultisoulApp;
use crate::canvas::master_layout::{enter_master_layout_for_area, switch_master_layout_for_area};
use canvas::NodeId;
use gpui::{AppContext, Context, Window};
use std::path::{Path, PathBuf};

impl MultisoulApp {
    /// 终端里点击的文件路径：先在 Agent worktree、再在项目根解析，成功后按 `system` 选择系统应用或 Norma 内预览。
    pub(crate) fn open_terminal_path(
        &mut self,
        targets: Vec<String>,
        system: bool,
        source_node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.runtime.workspace_root().map(Path::to_path_buf) else {
            self.workspace_state.error = Some(ui::i18n::LocalizedText::new("preview.open_project"));
            return;
        };
        let mut bases: Vec<PathBuf> = Vec::new();
        if let Some(worktree) = self.runtime.agent_worktree(source_node) {
            bases.push(worktree.to_path_buf());
        }
        if !bases.contains(&root) {
            bases.push(root.clone());
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let (areas, preferred) = self.desktop_area_context(cx).unwrap_or_default();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let hit = resolve_terminal_path(&bases, home.as_deref(), &targets)?;
                    Ok::<_, String>(plan_terminal_path(
                        hit,
                        &areas,
                        preferred.as_deref(),
                        system,
                    ))
                })
                .await;
            this.update_in(cx, |app, window, cx| {
                if app.runtime.workspace_root() != Some(root.as_path())
                    || app.runtime.document.node(source_node).is_none()
                {
                    return;
                }
                match result {
                    Ok(plan) => {
                        app.workspace_state.error = None;
                        app.present_terminal_path(plan, window, cx);
                    }
                    Err(error) => {
                        app.workspace_state.error = Some(
                            ui::i18n::LocalizedText::new("common.operation_failed")
                                .with_arg("reason", error),
                        )
                    }
                }
                cx.notify();
            })?;
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    fn present_terminal_path(
        &mut self,
        plan: TerminalPathPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TerminalPathPlan {
            hit,
            route,
            location,
        } = plan;
        match route {
            TerminalPathRoute::System => open_path_in_system_app(hit.path),
            TerminalPathRoute::Workbench => {
                let opened = location.is_some_and(|(area, rel)| {
                    self.open_workbench_file_in_host(area, rel, hit.line.map(|l| l as usize), cx)
                });
                if !opened {
                    open_path_in_system_app(hit.path);
                }
            }
            TerminalPathRoute::Directory => {
                let revealed = location
                    .is_some_and(|(area, rel)| self.reveal_workbench_dir_in_host(area, rel, cx));
                if !revealed {
                    open_path_in_system_app(hit.path);
                }
            }
            TerminalPathRoute::PreviewTab => {
                if self.can_create_preview() {
                    self.open_resolved_terminal_preview(hit.path, window, cx);
                }
            }
        }
    }

    fn open_resolved_terminal_preview(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("文件")
            .to_string();
        if self.route_project_content_to_desktop(
            storage::DesktopContentKind::File,
            path.to_string_lossy().into_owned(),
            title,
            cx,
        ) {
            return;
        }
        let area = self.visible_canvas_rect(window);
        let bounds = area.inset(self.layout_gap_canvas());
        let zoom = self.runtime.document.viewport.zoom;
        let node_id = self.runtime.spawn_preview_from_path(path, None, bounds);
        self.motion.spawn_node(node_id, bounds);
        let arranged = if self.master_layout.is_active() {
            switch_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        } else {
            enter_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        };
        if arranged {
            self.mark_canvas_dirty();
            self.load_preview_node(node_id, cx);
        } else {
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("preview.arrange_failed"));
        }
    }
}

/// 后台一次性算好的呈现方案：解析结果、分流目标与（工作台用的）项目归属。
pub(super) struct TerminalPathPlan {
    hit: TerminalPathHit,
    route: TerminalPathRoute,
    location: Option<(String, String)>,
}

fn plan_terminal_path(
    hit: TerminalPathHit,
    areas: &[(String, PathBuf)],
    preferred: Option<&str>,
    system: bool,
) -> TerminalPathPlan {
    if system {
        return TerminalPathPlan {
            hit,
            route: TerminalPathRoute::System,
            location: None,
        };
    }
    let canonical = std::fs::canonicalize(&hit.path).unwrap_or_else(|_| hit.path.clone());
    let roots: Vec<_> = areas
        .iter()
        .map(|(id, root)| {
            (
                id.clone(),
                std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()),
            )
        })
        .collect();
    let location = locate_in_areas(&canonical, &roots, preferred);
    let route = match inspect_path(&canonical) {
        Ok(facts) => route_terminal_path(&canonical, &facts, location.is_some()),
        Err(_) => TerminalPathRoute::System,
    };
    TerminalPathPlan {
        hit,
        route,
        location,
    }
}

pub(crate) fn open_path_in_system_app(path: PathBuf) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::thread::Builder::new()
            .name("open_terminal_path_in_system_app".to_string())
            .spawn(move || {
                let mut command = std::process::Command::new("open");
                if probe_system_open(&path) == SystemOpen::RevealInFinder {
                    command.arg("-R");
                }
                if let Err(error) = command.arg(&path).spawn() {
                    tracing::warn!(%error, "failed to open terminal path in system app");
                }
            });
    }
    #[cfg(not(target_os = "macos"))]
    drop(path);
}
