use super::*;
use crate::app::background_project::PreparedProject;
use gpui::{AppContext as _, TestAppContext};
use std::sync::{Arc, Mutex};

/// 桌面创建 Agent：已选择普通项目目录后直接启动，不等待 worktree 确认。
///
/// 数据构造：1 个桌面工作空间、1 个非 Git 项目、0 张旧卡片；快速 Codex + 自定义 Claude = 2 张。
/// 执行过程：建立隔离实例 → 快速创建 → 校验 PTY 参数 → 从自定义 Agent 定义创建 → 再次校验。
/// 预期结果：两个入口直接得到对应 Agent；cwd 是所选目录，不能退回父目录或停在选择器。
#[test]
fn desktop_agent_creation_uses_area_root_without_confirmation() {
    let temp = tempfile::tempdir().expect("应创建隔离测试目录");
    let root = temp.path().join("plain-project");
    std::fs::create_dir(&root).expect("应创建普通项目");
    let root = root.canonicalize().expect("应规范化所选目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Test").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加普通项目");
    let prepared =
        PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("应准备项目实例");
    let launches = Arc::new(Mutex::new(Vec::new()));
    let captured = launches.clone();
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(host.downgrade());
        app.runtime
            .set_pty_factory(Box::new(move |command, _, _, tx| {
                captured
                    .lock()
                    .expect("启动记录锁")
                    .push(command.cwd.clone());
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
            }));
        app
    });
    host.update(cx, |app, _| {
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.expanded_area_id = Some(area.id.clone());
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    session.update_in(cx, |app, window, cx| {
        app.spawn_agent_card(canvas::AgentProfile::Codex, window, cx);
    });
    cx.run_until_parked();

    let first_launches = launches.lock().expect("应读取启动记录");
    assert_eq!(
        first_launches.len(),
        1,
        "一次创建必须直接启动 PTY，不能停在 worktree 选择器"
    );
    assert_eq!(
        first_launches[0].as_deref(),
        Some(root.as_path()),
        "Agent cwd 必须是已选项目目录"
    );
    assert_ne!(
        first_launches[0].as_deref(),
        root.parent(),
        "不能退回项目父目录"
    );
    drop(first_launches);
    session.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 1, "目标项目必须出现一张真实 Agent 卡片");
        assert!(matches!(&app.runtime.document.nodes()[0].kind, canvas::CanvasNodeKind::Agent(spec) if spec.profile == canvas::AgentProfile::Codex), "新卡片应为用户选择的 Codex");
        assert!(!app.runtime.document.nodes().iter().any(|node| matches!(&node.kind, canvas::CanvasNodeKind::Term(_))), "不得用 Terminal 假卡片代替 Agent");
    });
    let custom = session.update(cx, |app, _| {
        app.agent_definitions
            .create_definition(Some("Reviewer".into()), canvas::AgentProfile::Claude)
            .expect("应创建自定义 Agent 定义")
    });
    session.update_in(cx, |app, window, cx| {
        app.spawn_agent_card_from_definition(custom.id, window, cx);
    });
    cx.run_until_parked();
    let launches = launches.lock().expect("自定义创建启动记录");
    assert_eq!(
        launches.len(),
        2,
        "自定义入口也应直接启动，不能再次要求选目录"
    );
    assert_eq!(
        launches[1].as_deref(),
        Some(root.as_path()),
        "自定义 Agent 同样使用所属项目目录"
    );
    session.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 2, "两个创建入口应得到两张卡片");
        assert!(matches!(&app.runtime.document.nodes()[1].kind, canvas::CanvasNodeKind::Agent(spec) if spec.definition_id == Some(custom.id) && spec.profile == canvas::AgentProfile::Claude), "第二张卡片必须保留用户选择的自定义定义");
    });
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("Git fixture 应可执行");
    assert!(
        output.status.success(),
        "Git fixture 应成功：{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 已选 linked worktree：导航中完成创建仍归原项目，同目录跨空间各有独立卡片；移除后不再启动。
///
/// 数据构造：1 个 Git 主目录 + 1 个 linked worktree；2 个空间各有 1 个区域，均选择 linked。
///   前两次请求分别创建 Codex / Claude，预期 1 + 1 = 2 次 PTY 启动；第三次移除目标，新增数为 0。
/// 执行过程：A 请求后立即导航 B → 推进校验 → 在 B 再创建 → A 再请求但移除 session → 推进校验。
/// 预期结果：A、B 各一张指定类型的卡片，cwd 均是 linked；无主目录回退、无导航串卡、无移除后启动。
#[test]
fn desktop_agent_creation_preserves_worktree_and_origin_across_navigation() {
    let temp = tempfile::tempdir().expect("应创建隔离 Git fixture");
    let main = temp.path().join("main");
    let linked = temp.path().join("linked");
    std::fs::create_dir(&main).expect("应创建主目录");
    git(&main, &["init", "-q"]);
    git(
        &main,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "selected",
            linked.to_str().expect("fixture 路径"),
        ],
    );
    let linked = linked.canonicalize().expect("应规范化 linked worktree");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let catalog = store.desktop_workspace_store();
    let workspace_a = catalog.create("A").expect("应创建 A");
    let workspace_b = catalog.create("B").expect("应创建 B");
    let area_a = catalog
        .add_project(&workspace_a.id, &linked)
        .expect("A 选择 linked");
    let area_b = catalog
        .add_project(&workspace_b.id, &linked)
        .expect("B 选择相同 linked");
    let prepared_a =
        PreparedProject::load_for_desktop(paths.clone(), area_a.clone()).expect("准备 A");
    let prepared_b =
        PreparedProject::load_for_desktop(paths.clone(), area_b.clone()).expect("准备 B");
    let launches = Arc::new(Mutex::new(Vec::new()));
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    let mut create_session = |prepared| {
        let captured = launches.clone();
        cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.runtime
                .set_pty_factory(Box::new(move |command, _, _, tx| {
                    captured
                        .lock()
                        .expect("启动记录锁")
                        .push(command.cwd.clone());
                    Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
                }));
            app
        })
    };
    let session_a = create_session(prepared_a);
    let session_b = create_session(prepared_b);
    let key_a = (workspace_a.id.clone(), area_a.id.clone());
    let key_b = (workspace_b.id.clone(), area_b.id.clone());
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = vec![workspace_a.clone(), workspace_b.clone()];
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone()];
        app.desktop_workspace
            .sessions
            .insert(key_a.clone(), session_a.clone());
        app.desktop_workspace
            .sessions
            .insert(key_b, session_b.clone());
        app.begin_desktop_agent_creation_for_area(
            area_a.id.clone(),
            canvas::AgentProfile::Codex,
            None,
            window,
            cx,
        );
        app.desktop_workspace.active_id = Some(workspace_b.id.clone());
        app.desktop_workspace.areas = vec![area_b.clone()];
    });
    cx.run_until_parked();
    session_a.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 1, "导航后原项目 A 仍应完成创建");
        assert!(matches!(&app.runtime.document.nodes()[0].kind, canvas::CanvasNodeKind::Agent(spec) if spec.profile == canvas::AgentProfile::Codex), "A 应保留 Codex 请求");
    });
    session_b.read_with(cx, |app, _| {
        assert!(
            app.runtime.document.nodes().is_empty(),
            "A 的异步结果不能投递到新选中的 B"
        );
    });
    host.update_in(cx, |app, window, cx| {
        app.begin_desktop_agent_creation(
            session_b.entity_id(),
            canvas::AgentProfile::Claude,
            None,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    session_b.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 1, "相同目录在 B 中仍应能创建独立卡片");
        assert!(matches!(&app.runtime.document.nodes()[0].kind, canvas::CanvasNodeKind::Agent(spec) if spec.profile == canvas::AgentProfile::Claude), "B 应得到自己的 Claude 卡片");
    });
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone()];
        app.begin_desktop_agent_creation_for_area(
            area_a.id.clone(),
            canvas::AgentProfile::Codex,
            None,
            window,
            cx,
        );
        app.desktop_workspace.sessions.remove(&key_a);
    });
    cx.run_until_parked();
    let launches = launches.lock().expect("应读取启动记录");
    assert_eq!(launches.len(), 2, "目标移除后第三次创建不能再启动 PTY");
    assert_eq!(
        launches[0].as_deref(),
        Some(linked.as_path()),
        "A 必须使用已选 linked worktree"
    );
    assert_eq!(
        launches[1].as_deref(),
        Some(linked.as_path()),
        "B 必须使用相同目录而拥有独立会话"
    );
    assert_ne!(
        launches[0].as_deref(),
        Some(main.as_path()),
        "不得自动退回仓库主目录"
    );
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace
                .error
                .as_deref()
                .is_some_and(|message| message.contains("try again")),
            "失效目标应给出可重试错误"
        );
    });
    assert!(
        !linked.join(".git").is_dir(),
        "所选目录必须仍是 linked worktree，不能被重建为新仓库"
    );
}

/// 创建前目录校验：拒绝目录消失、目录变成文件或项目身份被替换。
///
/// 数据构造：1 个普通目录和对应的规范化身份；分别变更 project_key 和文件类型。
/// 执行过程：有效目录校验成功 → 换成其他 key → 恢复 key 但移除目录 → 同路径创建文件。
/// 预期结果：仅原始目录通过；错误身份、缺失目录和普通文件不能触发 Agent 创建。
#[test]
fn desktop_agent_creation_rejects_unavailable_or_changed_directory() {
    let temp = tempfile::tempdir().expect("隔离校验目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建目录");
    let root = root.canonicalize().expect("目录应可规范化");
    let mut area = DesktopProjectArea {
        id: "area".into(),
        desktop_workspace_id: "workspace".into(),
        project_key: root.clone(),
        root: root.clone(),
        display_name: "Project".into(),
        added_at_unix_ms: 0, // 此测试不涉及时间排序，零是无时间语义的占位值。
    };
    assert!(
        validate_desktop_agent_root(&area).is_ok(),
        "有效普通项目必须可直接创建"
    );
    area.project_key = root.join("different-identity");
    assert!(
        validate_desktop_agent_root(&area).is_err(),
        "项目身份替换必须拒绝"
    );
    area.project_key = root.clone();
    std::fs::remove_dir(&root).expect("移除测试目录");
    assert!(
        validate_desktop_agent_root(&area).is_err(),
        "缺失目录必须拒绝"
    );
    std::fs::write(&root, "not a directory").expect("创建同名测试文件");
    assert!(
        validate_desktop_agent_root(&area).is_err(),
        "普通文件不得被当作 Agent cwd"
    );
}
