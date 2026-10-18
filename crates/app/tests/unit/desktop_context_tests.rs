use super::resolve_desktop_context;
use crate::app::AppScreen;
use agent::DesktopContext;
use storage::{DesktopProjectArea, DesktopWorkspace};

/// DESKTOP-CONTEXT-1：总览选择不构成当前项目，项目画布加载后才可进入提示词。
///
/// 数据构造：一个工作空间 dw-42 含项目 area-7；项目数据存在但总览没有展开画布。
/// 执行过程：先解析总览，再解析已加载的项目画布，最后返回启动页。
/// 预期结果：总览仅保留工作空间；画布带项目稳定 ID；启动页不沿用旧工作空间。
#[test]
fn visible_desktop_context_distinguishes_overview_project_and_launchpad() {
    let workspace = DesktopWorkspace {
        id: "dw-42".into(),
        name: "产品研发".into(),
        created_at_unix_ms: 1,
        last_opened_at_unix_ms: 2,
        project_count: 1,
        expanded_area_id: None,
        overview_scroll_y: 0,
        wallpaper: None,
    };
    let areas = vec![DesktopProjectArea {
        id: "area-7".into(),
        desktop_workspace_id: "dw-42".into(),
        project_key: "/project".into(),
        root: "/project".into(),
        display_name: "Norma OS".into(),
        added_at_unix_ms: 3,
        wallpaper: None,
    }];

    let overview = resolve_desktop_context(
        AppScreen::DesktopWorkspace,
        Some(&workspace),
        &areas,
        None,
        false,
    );
    assert_eq!(
        overview,
        Some(DesktopContext::WorkspaceOverview {
            workspace_id: "dw-42".into(),
            workspace_name: "产品研发".into(),
        }),
        "总览应有当前工作空间，但不应沿用项目"
    );
    assert!(
        !matches!(overview, Some(DesktopContext::ProjectCanvas { .. })),
        "总览中项目列表存在也不能被当成聚焦项目"
    );

    let project = resolve_desktop_context(
        AppScreen::DesktopWorkspace,
        Some(&workspace),
        &areas,
        Some("area-7"),
        true,
    );
    assert_eq!(
        project,
        Some(DesktopContext::ProjectCanvas {
            workspace_id: "dw-42".into(),
            workspace_name: "产品研发".into(),
            area_id: "area-7".into(),
            project_name: "Norma OS".into(),
        }),
        "只有实际可见的项目画布才应给 Norma 当前项目身份"
    );

    let launchpad = resolve_desktop_context(
        AppScreen::Launchpad,
        Some(&workspace),
        &areas,
        Some("area-7"),
        true,
    );
    assert_eq!(
        launchpad,
        Some(DesktopContext::NoWorkspace),
        "回到启动页后，保留的 active_id 不能冒充当前打开工作空间"
    );
    assert!(
        !matches!(launchpad, Some(DesktopContext::ProjectCanvas { .. })),
        "启动页不能继续暴露上次项目为当前项目"
    );
}
