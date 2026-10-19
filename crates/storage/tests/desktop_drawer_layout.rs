use storage::DesktopWorkspaceStore;

/// 左栏视图偏好按桌面工作空间保存，重新打开数据库后仍在，且不串到别的工作空间。
///
/// 数据构造：两个空工作空间；概览 JSON 为运行时约定的 `{"left_view":"overview"}`。
/// 执行过程：只给 Alpha 写入布局，再用同一路径新建 Store 读取。
/// 预期结果：Alpha 读回这段 JSON；Beta 仍是空，说明偏好没有写到别的工作空间。
#[test]
fn drawer_layout_persists_per_desktop_workspace() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let alpha = store.create("Alpha").expect("应创建 Alpha");
    let beta = store.create("Beta").expect("应创建 Beta");
    assert_eq!(
        alpha.drawer_layout_json, None,
        "新建工作空间还没有左栏视图偏好"
    );
    assert_eq!(beta.drawer_layout_json, None, "另一个工作空间同样没有偏好");

    let saved = r#"{"left_view":"overview"}"#;
    store
        .set_drawer_layout(&alpha.id, saved)
        .expect("应写入 Alpha 的左栏视图");

    let reloaded = DesktopWorkspaceStore::new(path)
        .list()
        .expect("重启后应重新读取工作空间");
    let alpha_again = reloaded
        .iter()
        .find(|workspace| workspace.id == alpha.id)
        .expect("Alpha 应仍在");
    let beta_again = reloaded
        .iter()
        .find(|workspace| workspace.id == beta.id)
        .expect("Beta 应仍在");
    assert_eq!(
        alpha_again.drawer_layout_json.as_deref(),
        Some(saved),
        "Alpha 的运行概览偏好应在重新打开后还原"
    );
    assert_eq!(
        beta_again.drawer_layout_json, None,
        "Beta 不应继承 Alpha 的左栏视图"
    );
    assert_ne!(alpha.id, beta.id, "两个工作空间不得共享身份");
}

/// 悬浮状态和偏移随抽屉布局按工作空间往返，不会写到另一个工作空间。
///
/// 数据构造：两个空工作空间。悬浮 JSON 为运行时约定的
///   `{"floating":true,"left_view":"chat","offset_x":-40,"offset_y":20}`。
/// 执行过程：只给 Alpha 写入这段布局，再用同一路径新建 Store 读取。
/// 预期结果：Alpha 读回原文字；Beta 仍是空。
#[test]
fn floating_overview_layout_roundtrips_per_workspace() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let alpha = store.create("Alpha").expect("应创建 Alpha");
    let beta = store.create("Beta").expect("应创建 Beta");
    let saved = r#"{"floating":true,"left_view":"chat","offset_x":-40,"offset_y":20}"#;
    store
        .set_drawer_layout(&alpha.id, saved)
        .expect("应写入 Alpha 的悬浮布局");

    let reloaded = DesktopWorkspaceStore::new(path)
        .list()
        .expect("重启后应重新读取工作空间");
    let alpha_again = reloaded
        .iter()
        .find(|workspace| workspace.id == alpha.id)
        .expect("Alpha 应仍在");
    let beta_again = reloaded
        .iter()
        .find(|workspace| workspace.id == beta.id)
        .expect("Beta 应仍在");
    assert_eq!(
        alpha_again.drawer_layout_json.as_deref(),
        Some(saved),
        "Alpha 的悬浮偏移应在重新打开后原样还原"
    );
    assert_eq!(
        beta_again.drawer_layout_json, None,
        "Beta 不应继承 Alpha 的悬浮位置"
    );
    assert_ne!(alpha.id, beta.id, "两个工作空间不得共享身份");
}
