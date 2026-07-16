//! E2E-15：Recent projects 原生右键菜单删除。

use gpui::{point, px, Modifiers, MouseButton, TestAppContext};
use storage::RecentStore;
use workspace::{RecentWorkspaces, WorkspaceRecord, WorkspaceSource};

use crate::harness::{echo_factory, make_app};

/// Recent projects 右键菜单：删除精确项目，启动页列表与 SQLite 必须同步更新。
///
/// 场景：用户在启动页 Recent projects 的 beta 行右击，看到原生菜单中的 Remove，点击后
/// beta 从列表消失且重启时读取的 SQLite 也不再包含它。
///
/// 数据构造（含关键数值的推导过程）：
///   alpha path = temp/alpha-workspace，真实目录，opened_at=20，因此第 0 行
///   beta path  = temp/beta-workspace，真实目录，opened_at=10，因此第 1 行
///   初始项目数 = 2；删除目标数 = 1；预期剩余项目数 = 2 - 1 = 1
///   PopupMenu 第一项高度 = 26px，菜单锚定于右击坐标，点击锚点 y+13px 的首项中线
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将两条 canonical 最近项目写入临时 app.sqlite，并装配到 headless Launchpad。
///   2. 通过 beta 行真实 debug bounds 投递 Right mouse down/up，等待 gpui-component 创建菜单。
///   3. 点击 popup-menu 的唯一 Remove 项，读取 app e2e 投影与临时 SQLite。
///
/// 预期结果：
///   - 正断言：右击后的首个原生菜单项可删除 beta，alpha 在内存与 SQLite 中都存在。
///   - 负断言：beta 不应继续存在于内存列表或 SQLite，删除不应误伤 alpha。
#[test]
fn e2e_15_recent_project_context_menu_removes_selected_project_and_persists() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temp = tempfile::tempdir().expect("应创建隔离的 Recent projects e2e 临时目录");
    let alpha = temp.path().join("alpha-workspace");
    let beta = temp.path().join("beta-workspace");
    std::fs::create_dir(&alpha).expect("应创建 alpha workspace");
    std::fs::create_dir(&beta).expect("应创建 beta workspace");
    let alpha = alpha
        .canonicalize()
        .expect("alpha workspace 应解析为 canonical path");
    let beta = beta
        .canonicalize()
        .expect("beta workspace 应解析为 canonical path");
    let recent = RecentWorkspaces {
        records: vec![
            WorkspaceRecord::new(alpha.clone(), 20, WorkspaceSource::Open),
            WorkspaceRecord::new(beta.clone(), 10, WorkspaceSource::Open),
        ],
    };
    let store = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    app.update(cx, |app, cx| {
        app.set_recent_projects_for_e2e(store.clone(), recent, cx)
            .expect("e2e 应将两条 recent 项目写入隔离 SQLite");
    });
    cx.run_until_parked();

    let beta_row = cx
        .debug_bounds("launchpad-recent-project-1")
        .expect("beta Recent project 行应渲染稳定 debug id");
    let right_click = point(
        beta_row.origin.x + px(16.0),
        beta_row.origin.y + beta_row.size.height / 2.0,
    );
    cx.simulate_mouse_down(right_click, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(right_click, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();

    // gpui-component 的 PopupMenu 没有 debug_selector；ContextMenuExt 将它以 TopLeft
    // anchor 放在右击坐标。唯一菜单项是 Remove，26px 高度的中线为 y + 13px。
    let remove_click = point(right_click.x + px(12.0), right_click.y + px(13.0));
    cx.simulate_click(remove_click, Modifiers::none());
    cx.run_until_parked();

    let memory_paths = app.update(cx, |app, _| app.recent_project_paths_for_e2e());
    let persisted = store.load().expect("点击 Remove 后应能重新读取隔离 SQLite");

    assert_eq!(
        memory_paths,
        vec![alpha.clone()],
        "点击 beta 的 Remove 后启动页内存列表应只保留 alpha"
    );
    assert_eq!(
        persisted.records.len(),
        1,
        "点击 Remove 后 SQLite recent_workspaces 应从两条精确变为一条"
    );
    assert_eq!(
        persisted.records[0].path, alpha,
        "删除 beta 不应误伤 alpha 的 SQLite 记录"
    );
    assert!(
        !memory_paths.iter().any(|path| path == &beta),
        "beta 删除后不应残留在启动页 Recent projects 内存列表"
    );
    assert!(
        !persisted.records.iter().any(|record| record.path == beta),
        "beta 删除后不应残留在 SQLite recent_workspaces"
    );
}
