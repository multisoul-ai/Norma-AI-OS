fn desktop_content_tool_available(app: &MultisoulApp) -> bool {
    app.desktop_host.is_some()
        || matches!(app.screen, crate::app::AppScreen::DesktopWorkspace)
}

fn open_desktop_browser_tab_from_tool(
    app: &mut MultisoulApp,
    url: String,
    cx: &mut Context<MultisoulApp>,
) -> Result<AdapterResult, AdapterError> {
    let title = url::Url::parse(&url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "浏览器".into());
    app.norma_open_desktop_content_tab(storage::DesktopContentKind::Browser, url, title, cx)?;
    Ok(AdapterResult::accepted("正在右侧打开网页标签"))
}

fn open_desktop_preview_tab_from_tool(
    app: &mut MultisoulApp,
    path: std::path::PathBuf,
    cx: &mut Context<MultisoulApp>,
) -> Result<AdapterResult, AdapterError> {
    let project_root = app
        .runtime
        .workspace_root()
        .map(std::path::Path::to_path_buf)
        .or_else(|| {
            app.desktop_workspace.expanded_area_id.as_ref().and_then(|id| {
                app.desktop_workspace
                    .areas
                    .iter()
                    .find(|area| &area.id == id)
                    .map(|area| area.root.clone())
            })
        })
        .ok_or_else(|| AdapterError::Rejected("请先聚焦项目，再打开项目文件".into()))?;
    let path = crate::preview::resolve_preview_path(
        &project_root,
        path.to_string_lossy().as_ref(),
    )
    .map_err(|error| AdapterError::Rejected(error.to_string()))?;
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "文件".into());
    app.norma_open_desktop_content_tab(
        storage::DesktopContentKind::File,
        path.to_string_lossy().into_owned(),
        title,
        cx,
    )?;
    Ok(AdapterResult::accepted("正在右侧打开文件标签"))
}
