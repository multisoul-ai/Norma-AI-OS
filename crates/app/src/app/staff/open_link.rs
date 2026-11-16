//! 幕僚富文本里的链接/路径点击：外链进抽屉浏览器标签，本地路径按终端规则在抽屉预览。

use crate::app::MultisoulApp;
use crate::preview::rich_link::{classify_target, RichTarget};
use crate::preview::terminal_open::{open_path_in_system_app, plan_terminal_path};
use crate::preview::terminal_path::resolve_terminal_path;
use crate::preview::terminal_route::TerminalPathRoute;
use gpui::{App, AppContext, Context, Global, WeakEntity, Window, WindowId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use storage::DesktopContentKind;
use ui::component::text::LinkClickHandler;

/// 窗口 → 渲染幕僚的 `MultisoulApp`。窗口根视图不是 `MultisoulApp`，不能靠根视图向下转型。
#[derive(Default)]
struct LinkTargets(HashMap<WindowId, WeakEntity<MultisoulApp>>);

impl Global for LinkTargets {}

/// 幕僚富文本渲染时登记所在窗口，点击回调据此找回应用实体。
pub(crate) fn register_link_target(window: &Window, cx: &mut Context<MultisoulApp>) {
    let id = window.window_handle().window_id();
    let weak = cx.weak_entity();
    cx.default_global::<LinkTargets>().0.insert(id, weak);
}

/// 接管 `TextView` 的链接点击；窗口没有登记目标时回退系统打开。
pub(crate) fn install_link_handler(cx: &mut App) {
    cx.set_global(LinkClickHandler(Rc::new(|url, window, cx| {
        let target = cx
            .try_global::<LinkTargets>()
            .and_then(|targets| targets.0.get(&window.window_handle().window_id()))
            .and_then(WeakEntity::upgrade);
        match target {
            Some(app) => {
                let url = url.to_owned();
                app.update(cx, |app, cx| app.open_staff_link(&url, window, cx));
            }
            None => cx.open_url(url),
        }
    })));
}

impl MultisoulApp {
    /// 渲染幕僚的可能是项目会话实体；工作空间、项目区域与内容抽屉都在宿主上，先转给宿主。
    pub(crate) fn open_staff_link(
        &mut self,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| host.open_staff_link_here(url, window, cx));
        } else {
            self.open_staff_link_here(url, window, cx);
        }
    }

    fn open_staff_link_here(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let area = self.staff_area_id().map(str::to_owned);
        match classify_target(url) {
            RichTarget::Web(url) => self.open_desktop_content_tab(
                DesktopContentKind::Browser,
                url.clone(),
                url,
                area,
                cx,
            ),
            RichTarget::System(url) => cx.open_url(&url),
            RichTarget::Path(raw) => self.open_staff_path(raw, area, window, cx),
            RichTarget::Reject => {}
        }
    }

    fn open_staff_path(
        &mut self,
        raw: String,
        area: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let areas: Vec<(String, PathBuf)> = self
            .desktop_workspace
            .areas
            .iter()
            .map(|a| (a.id.clone(), a.root.clone()))
            .collect();
        // 当前任务的项目优先；任务没有关联项目时仍要能解析绝对路径，所以退到其余项目，最后退到根。
        let mut bases: Vec<PathBuf> = areas
            .iter()
            .filter(|(id, _)| Some(id) == area.as_ref())
            .map(|(_, root)| root.clone())
            .collect();
        bases.extend(
            areas
                .iter()
                .filter(|(id, _)| Some(id) != area.as_ref())
                .map(|(_, root)| root.clone()),
        );
        if bases.is_empty() {
            bases.push(PathBuf::from("/"));
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let preferred = area.clone();
        cx.spawn_in(window, async move |this, cx| {
            // 路径存在性只在点击后台校验，渲染路径不碰磁盘。
            let plan = cx
                .background_spawn(async move {
                    let hit = resolve_terminal_path(&bases, home.as_deref(), &[raw])?;
                    Ok::<_, String>(plan_terminal_path(hit, &areas, preferred.as_deref(), false))
                })
                .await;
            this.update(cx, |app, cx| {
                let Ok(plan) = plan else {
                    // 幕僚界面不渲染 desktop_workspace.error，用幕僚自己的底部提示反馈。
                    app.show_staff_toast(ui::i18n::message("drawers.file_moved", &[]), None, cx);
                    return;
                };
                let path = plan.hit.path;
                let title = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let resource = path.to_string_lossy().into_owned();
                let source_area = plan
                    .location
                    .as_ref()
                    .map(|(area, _)| area.clone())
                    .or_else(|| app.staff_area_id().map(str::to_owned));
                // 幕僚里没有文件工作台面板，项目内文件也走右侧内容抽屉，与验收产物一致。
                match (plan.route, plan.location) {
                    (TerminalPathRoute::Directory, Some((area, rel))) => {
                        app.reveal_dir_in_workbench(area, rel, cx);
                    }
                    (TerminalPathRoute::System | TerminalPathRoute::Directory, _) => {
                        if !crate::preview::rich_link::is_unsafe_for_system_open(&path) {
                            open_path_in_system_app(path)
                        }
                    }
                    (TerminalPathRoute::PreviewTab | TerminalPathRoute::Workbench, _) => app
                        .open_desktop_content_tab(
                            DesktopContentKind::File,
                            resource,
                            title,
                            source_area,
                            cx,
                        ),
                }
            })
        })
        .detach();
    }
}
