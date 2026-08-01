use app::{AppScreen, CloneProject};
use gpui::{rgba, Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-21-color：克隆弹窗的真实 Input 必须使用 Norma 当前主题的可见正文色。
///
/// 数据构造（含关键数值的推导过程）：
///   stale component foreground = `0xffffffff`（深色系统主题的纯白正文）
///   Clone modal theme = Paper Blue(1)；Repository InputState = 1。
///   预期主题同步次数 = App 构造时 1 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先初始化 gpui-component，并注入纯白 foreground，模拟 macOS 深色外观残留。
///   2. 构造真实 MultisoulApp；启动流程把默认 Paper Blue 同步到组件主题。
///   3. 点击 Launchpad 的 Clone Repo，打开包含 gpui-component Input 的真实弹窗。
///   4. 读取 Input 绘制实际使用的全局正文色、占位色和主题模式。
///
/// 预期结果：
///   - 正断言：弹窗打开，正文色与占位色分别匹配 `text_primary` / `text_secondary`。
///   - 正断言：组件模式为 Light，与浅色克隆面板一致。
///   - 负断言：正文色不再保留纯白色，否则已输入 URL 会融入输入框背景。
#[test]
fn github_clone_dialog_uses_norma_text_colors_in_component_input() {
    let mut test_cx = TestAppContext::single();
    test_cx.update(|cx| {
        ui::init_components(cx);
        ui::component::Theme::global_mut(cx).colors.foreground = rgba(0xffffffff).into();
    });
    let (app, cx) = make_app(&mut test_cx, echo_factory());

    click_debug(cx, "launchpad-clone-repo");
    let opened = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    let (foreground, muted_foreground, mode) = app.update(cx, |_app, cx| {
        let theme = ui::component::Theme::global(cx);
        (
            theme.colors.foreground,
            theme.colors.muted_foreground,
            theme.mode,
        )
    });
    let norma_theme = ui::Theme::paper_blue();

    assert!(opened.open, "颜色契约检查前 Clone modal 必须已经打开");
    assert_eq!(
        foreground,
        rgba(norma_theme.text_primary).into(),
        "Repository Input 的非空正文必须使用 Paper Blue text_primary"
    );
    assert_eq!(
        muted_foreground,
        rgba(norma_theme.text_secondary).into(),
        "Repository Input 的占位文字必须使用 Paper Blue text_secondary"
    );
    assert_eq!(
        mode,
        ui::component::ThemeMode::Light,
        "浅色 Clone modal 必须令组件库处于 Light 模式"
    );
    assert_ne!(
        foreground,
        rgba(0xffffffff).into(),
        "浅色输入框的非空正文不得继续使用不可见的纯白前景色"
    );
}

/// E2E-21：用户从 Launchpad 输入 GitHub URL、切换 SSH、指定目录并完成克隆后进入项目窗口。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口数 = 1（Launchpad）；克隆任务数 = 1；最终窗口数 = 1 - Launchpad(1) + Workspace(1) = 1。
///   输入字段 = repository URL(1) + optional folder(1) = 2 个真实 InputState。
///   fake clone 产物 = README(1)；Workspace 初始化产物 = project.test.sqlite(1)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 点击真实 `launchpad-clone-repo` hitbox，打开 Clone Project modal。
///   2. 经平台文本输入填入 HTTPS URL，点击 SSH 分段按钮，再填本地目录名。
///   3. 将目标父目录切到隔离 fixture，点击真实 Clone 按钮。
///   4. 后台 fake runner 物化仓库；完成回调以 WorkspaceSource::Clone 新建项目窗口并关闭 Launchpad。
///   5. 从隔离 app SQLite 读取 Recent，并检查项目目录与测试 ProjectStore。
///
/// 预期结果：
///   - 正断言：modal 可见，输入草稿完整，传输方式切换为 SSH，目标预览精确。
///   - 正断言：Recent 记录 source=Clone，README 与 project.test.sqlite 均存在。
///   - 负断言：最终不保留 Launchpad 双窗口，不创建生产 project.sqlite，也不记录错误目录。
#[test]
fn github_clone_launchpad_flow_opens_cloned_workspace_and_records_clone_source() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("GitHub Clone E2E 应创建隔离 fixture");
    let projects = fixture.path().join("projects");
    let unrelated = fixture.path().join("unrelated");
    std::fs::create_dir(&projects).expect("GitHub Clone E2E 应创建目标父目录");
    std::fs::create_dir(&unrelated).expect("GitHub Clone E2E 应创建负对照目录");
    let app_database_path = app.read_with(cx, |app, _cx| app.app_database_path_for_e2e());
    app.update(cx, |app, _cx| {
        app.use_successful_clone_backend_for_e2e();
    });

    click_debug(cx, "launchpad-clone-repo");
    let opened = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert!(opened.open, "点击 Clone Repo 后 modal 必须处于 open 状态");
    assert_eq!(
        opened.phase, "editing",
        "新打开的 Clone modal 应处于 editing，而不是提前运行任务"
    );
    assert_ne!(
        opened.phase, "running",
        "尚未输入 URL 时不得提前启动 Git clone"
    );

    app.update(cx, |app, cx| {
        app.set_clone_parent_for_e2e(projects.clone(), cx);
    });
    click_debug(cx, "clone-project-remote-input");
    cx.simulate_input("https://github.com/openai/codex.git");
    click_debug(cx, "clone-transport-ssh");
    click_debug(cx, "clone-project-folder-input");
    cx.simulate_input("codex-e2e");
    cx.run_until_parked();

    let ready = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    let expected_target = projects.join("codex-e2e");
    assert_eq!(
        ready.remote, "https://github.com/openai/codex.git",
        "Repository InputState 应保存完整 HTTPS URL"
    );
    assert_eq!(
        ready.folder, "codex-e2e",
        "Folder InputState 应保存用户覆盖的目录名"
    );
    assert_eq!(
        ready.transport, "ssh",
        "点击 SSH 分段按钮后实际请求应切换到 SSH"
    );
    assert_eq!(
        ready.target_preview.as_deref(),
        Some(expected_target.as_path()),
        "目标预览应精确等于 fixture/projects/codex-e2e"
    );
    assert_ne!(
        ready.target_preview.as_deref(),
        Some(unrelated.as_path()),
        "目标预览不得指向未选择的负对照目录"
    );

    click_debug(cx, "clone-project-submit");
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let canonical_target = expected_target
        .canonicalize()
        .expect("fake clone 完成后目标应可 canonicalize");
    let canonical_unrelated = unrelated
        .canonicalize()
        .expect("负对照目录应可 canonicalize");
    let recent = storage::RecentStore::new(app_database_path)
        .load()
        .expect("GitHub Clone E2E 应从隔离 app DB 读取 Recent");
    let cloned_record = recent
        .records
        .iter()
        .find(|record| record.path == canonical_target)
        .expect("Recent 必须包含克隆后的 canonical target");

    assert_eq!(
        cloned_record.source,
        workspace::WorkspaceSource::Clone,
        "克隆完成后 Recent source 必须是 Clone，而不是 Open"
    );
    assert!(
        canonical_target.join("README.md").is_file(),
        "后台 clone backend 应物化 README 作为仓库内容证据"
    );
    assert!(
        canonical_target
            .join(".multisoul")
            .join("project.test.sqlite")
            .is_file(),
        "自动打开克隆项目后应初始化隔离的 project.test.sqlite"
    );
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "克隆成功后应关闭 Launchpad，仅保留一个项目窗口"
    );
    assert!(
        !recent
            .records
            .iter()
            .any(|record| record.path == canonical_unrelated),
        "Recent 不得记录未选择的 unrelated 负对照目录"
    );
    assert!(
        !canonical_target
            .join(".multisoul")
            .join("project.sqlite")
            .exists(),
        "E2E 克隆流程不得创建生产 project.sqlite"
    );
}

/// E2E-21a：从已有 Workspace 克隆成功后应保留当前工作现场，并在第二个窗口打开项目。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口数 = 1（existing Canvas）；新增项目窗口数 = 1；最终窗口数 = 1 + 1 = 2。
///   existing workspace = 1；clone target = 1；Recent Clone 记录 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 existing workspace Canvas，通过 File 菜单 Action 打开 Clone modal。
///   2. 输入 GitHub HTTPS URL、隔离目标父目录，并用 fake backend 完成 clone。
///   3. 完成回调以 close_current_launchpad=false 创建第二个项目窗口。
///   4. 检查原 app screen/runtime root、窗口数量和 Recent Clone 记录。
///
/// 预期结果：
///   - 正断言：新克隆项目在第二个窗口打开，Recent source=Clone。
///   - 正断言：原窗口仍为 Canvas 且 runtime root 仍是 existing workspace。
///   - 负断言：不得关闭或替换原 Workspace，也不得把 existing 误记为 Clone。
#[test]
fn github_clone_from_workspace_opens_second_window_without_replacing_current_canvas() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("Workspace Clone E2E 应创建隔离 fixture");
    let existing = fixture.path().join("existing");
    let projects = fixture.path().join("projects");
    std::fs::create_dir(&existing).expect("Workspace Clone E2E 应创建 existing workspace");
    std::fs::create_dir(&projects).expect("Workspace Clone E2E 应创建 clone 父目录");
    enter_canvas(&app, cx, &existing);
    let existing_root = existing
        .canonicalize()
        .expect("existing workspace 应可 canonicalize");
    let app_database_path = app.read_with(cx, |app, _cx| app.app_database_path_for_e2e());
    app.update(cx, |app, _cx| {
        app.use_successful_clone_backend_for_e2e();
    });

    cx.dispatch_action(CloneProject);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.set_clone_parent_for_e2e(projects.clone(), cx);
    });
    click_debug(cx, "clone-project-remote-input");
    cx.simulate_input("https://github.com/openai/codex.git");
    click_debug(cx, "clone-project-folder-input");
    cx.simulate_input("codex-second-window");
    click_debug(cx, "clone-project-submit");
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let target = projects
        .join("codex-second-window")
        .canonicalize()
        .expect("fake clone target 应可 canonicalize");
    let retained_root = read_runtime(&app, cx, |runtime| {
        runtime.workspace_root().map(std::path::Path::to_path_buf)
    });
    let recent = storage::RecentStore::new(app_database_path)
        .load()
        .expect("Workspace Clone E2E 应读取隔离 Recent");
    let cloned_record = recent
        .records
        .iter()
        .find(|record| record.path == target)
        .expect("Recent 应包含第二窗口打开的 clone target");

    assert_eq!(
        cx.cx.windows().len(),
        2,
        "已有 Workspace 克隆成功后应保留原窗口并新增一个项目窗口"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.screen()),
        AppScreen::Canvas,
        "原 Workspace 窗口必须继续显示 Canvas"
    );
    assert_eq!(
        retained_root.as_deref(),
        Some(existing.as_path()),
        "原窗口 runtime root 不得被新 clone target 替换"
    );
    assert_ne!(
        retained_root.as_deref(),
        Some(target.as_path()),
        "新 clone target 必须在第二窗口打开，不能占用原 runtime"
    );
    assert_eq!(
        cloned_record.source,
        workspace::WorkspaceSource::Clone,
        "第二窗口项目仍必须以 WorkspaceSource::Clone 进入 Recent"
    );
    assert!(
        !recent.records.iter().any(|record| {
            record.path == existing_root && record.source == workspace::WorkspaceSource::Clone
        }),
        "existing workspace 不得因另一个项目的 clone 被误记为 Clone"
    );
}

/// E2E-21b：无效 host 应在 modal 内即时阻断提交，且取消后不留下目录或 Recent 记录。
///
/// 数据构造（含关键数值的推导过程）：
///   非 GitHub URL = `https://gitlab.com/openai/codex.git`(1)。
///   允许启动的 clone 任务数 = 0；预期创建目录数 = 0；预期 Recent 新增数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Launchpad 打开真实 Clone modal，并把父目录设为隔离 fixture。
///   2. 点击 Repository InputState，输入 gitlab.com URL。
///   3. 点击已禁用的 Clone 按钮，确认状态不进入 running。
///   4. 点击 Cancel 关闭 modal，再读取目标目录和隔离 Recent。
///
/// 预期结果：
///   - 正断言：显示 UnsupportedHost 文案且保持 editing。
///   - 正断言：Cancel 后 modal 关闭。
///   - 负断言：不得生成目标预览、项目目录或 Recent 记录。
#[test]
fn github_clone_invalid_host_stays_in_editor_without_starting_git() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let fixture = tempfile::tempdir().expect("无效 GitHub Clone E2E 应创建隔离 fixture");
    let projects = fixture.path().join("projects");
    std::fs::create_dir(&projects).expect("无效 GitHub Clone E2E 应创建目标父目录");
    let app_database_path = app.read_with(cx, |app, _cx| app.app_database_path_for_e2e());

    click_debug(cx, "launchpad-clone-repo");
    app.update(cx, |app, cx| {
        app.set_clone_parent_for_e2e(projects.clone(), cx);
    });
    click_debug(cx, "clone-project-remote-input");
    cx.simulate_input("https://gitlab.com/openai/codex.git");
    cx.run_until_parked();

    let invalid = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert_eq!(
        invalid.phase, "editing",
        "无效 host 应保持 editing，不能进入 running"
    );
    assert_eq!(
        invalid.validation_error.as_deref(),
        Some("Only github.com repositories are supported."),
        "gitlab.com URL 应显示可操作的 UnsupportedHost 文案"
    );
    assert!(
        invalid.target_preview.is_none(),
        "无效远端不得生成看似可用的目标预览"
    );
    assert_ne!(invalid.phase, "running", "无效 URL 不能启动后台 Git");

    click_debug(cx, "clone-project-submit");
    let after_disabled_submit = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert_eq!(
        after_disabled_submit.phase, "editing",
        "点击禁用的 Clone 按钮后仍应保持 editing"
    );
    assert!(
        !projects.join("codex").exists(),
        "无效 host 不得创建 codex 目标目录"
    );

    click_debug(cx, "clone-project-cancel");
    let closed = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    let recent = storage::RecentStore::new(app_database_path)
        .load()
        .expect("无效 GitHub Clone E2E 应读取隔离 Recent");
    assert!(!closed.open, "点击 Cancel 后 Clone modal 必须关闭");
    assert!(
        recent.records.is_empty(),
        "未启动的无效 clone 不得写入任何 Recent 记录"
    );
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "取消无效 clone 后应只保留原 Launchpad 窗口"
    );
}

/// E2E-21c：粘贴 SSH 地址时协议选择应自动与地址保持一致。
///
/// 数据构造（含关键数值的推导过程）：
///   初始协议 = HTTPS(1)；粘贴地址协议 = SSH(1)；后台 clone 启动数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Launchpad 打开 Clone modal，确认默认协议是 HTTPS。
///   2. 在真实 Repository InputState 粘贴 `git@github.com:openai/codex.git`。
///   3. 等待 InputEvent::Change 完成解析，再读取 modal snapshot。
///
/// 预期结果：
///   - 正断言：协议自动切换为 SSH，地址草稿保持原样且目标目录名推导为 codex。
///   - 负断言：不得继续显示 HTTPS，也不得在未点击 Clone 时启动任务。
#[test]
fn github_clone_ssh_url_selects_ssh_transport_without_starting_clone() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());

    click_debug(cx, "launchpad-clone-repo");
    let initial = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert_eq!(
        initial.transport, "https",
        "新 Clone modal 的协议默认值必须是 HTTPS"
    );

    click_debug(cx, "clone-project-remote-input");
    cx.simulate_input("git@github.com:openai/codex.git");
    cx.run_until_parked();

    let parsed = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert_eq!(
        parsed.transport, "ssh",
        "粘贴有效 SSH URL 后协议选择必须自动切换为 SSH"
    );
    assert_eq!(
        parsed.remote, "git@github.com:openai/codex.git",
        "自动切换协议不得改写用户输入的 SSH 地址草稿"
    );
    assert!(
        parsed
            .target_preview
            .as_deref()
            .is_some_and(|path| path.ends_with("codex")),
        "SSH URL 应推导出以 codex 结尾的目标路径，实际为 {:?}",
        parsed.target_preview
    );
    assert_ne!(
        parsed.transport, "https",
        "有效 SSH URL 不得继续显示 HTTPS 选择态"
    );
    assert_eq!(
        parsed.phase, "editing",
        "仅粘贴 SSH URL 不得在用户确认前启动 clone"
    );
}

/// E2E-21d：File 菜单共享的 CloneProject Action 必须到达当前工作台并打开同一 modal。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 modal 数 = 0；Action dispatch 次数 = 1；预期 modal 数 = 0 + 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建已聚焦的 headless GPUI Launchpad root。
///   2. dispatch 原生 File 菜单绑定的公开 `CloneProject` Action。
///   3. Action 经 MultisoulApp root handler 创建 InputState 并打开 Clone modal。
///
/// 预期结果：
///   - 正断言：modal 打开且停留在 editing，默认协议为 HTTPS。
///   - 负断言：不得在只有菜单 Action 时启动后台 clone。
#[test]
fn github_clone_file_menu_action_opens_clone_modal() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let before = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert!(!before.open, "dispatch CloneProject 前 modal 不应已经打开");

    cx.dispatch_action(CloneProject);
    cx.run_until_parked();

    let opened = app.read_with(cx, |app, _cx| app.clone_project_e2e_snapshot());
    assert!(
        opened.open,
        "File 菜单 CloneProject Action 必须打开 Clone modal"
    );
    assert_eq!(
        opened.phase, "editing",
        "File 菜单打开的 modal 应等待用户输入，不能提前运行任务"
    );
    assert_eq!(
        opened.transport, "https",
        "File 菜单打开的 modal 应使用首次默认 HTTPS 协议"
    );
    assert_ne!(
        opened.phase, "running",
        "仅 dispatch 菜单 Action 不得启动后台 clone"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应拥有可点击 debug bounds"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
