use super::*;
use crate::canvas::master_layout::MasterLayoutState;
use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot,
    TermSpec, Vec2, Viewport,
};
use runtime::NodeView;
use storage::{
    LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot, RecentStore, SessionCheckpoint,
    SessionCheckpointKind,
};

/// 打开已有 workspace：app 层应先进入 Canvas，并把 live terminal 恢复延后执行。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace path = temp/restored-live-terminal，真实目录
///   canvas snapshot = [Term id=6,bounds(10,20,640,360)]
///   session checkpoint = node_id=6，shell="/bin/sh"，cols=88，rows=26
///   初始 runtime.live_len = 0，打开后仍为 0；pending restore node 数 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ProjectStore 写入 canvas snapshot 与 session checkpoint
///   2. enter_workspace_for_path(workspace)
///      → load_canvas_snapshot 恢复 document，但不同步 spawn terminal
///   3. 查询 pending restore，再手动执行 node_id=6 的单点恢复
///
/// 预期结果：
///   - 正断言：打开成功，screen == Canvas，pending restore 包含 1 个 node
///   - 正断言：手动执行单点 restore 后 node_view(6) 是 Term 且尺寸为 88x26
///   - 负断言：进入 Canvas 前不应同步恢复 live node，否则打开会被 PTY spawn 卡住
#[test]
fn test_enter_workspace_defers_live_terminal_restore_for_saved_card() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_path = temp.path().join("restored-live-terminal");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let project_store =
        ProjectStore::for_workspace(&workspace_path).expect("project store should open");
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 6,
            kind: CanvasNodeKind::Term(TermSpec),
            bounds: CanvasRect::new(10.0, 20.0, 640.0, 360.0),
            z_index: 2,
        }],
        viewport: Viewport {
            pan: Vec2::new(4.0, 5.0),
            zoom: 1.25,
        },
    };
    let checkpoint = SessionCheckpoint {
        node_id: 6,
        kind: SessionCheckpointKind::Terminal,
        payload: "Terminal".into(),
        cols: 88,
        rows: 26,
        shell: Some("/bin/sh".into()),
        agent_profile: None,
    };
    project_store
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("canvas snapshot 应写入 project store");
    project_store
        .session_store()
        .save_checkpoint(&checkpoint)
        .expect("session checkpoint 应写入 project store");
    let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut agent_definitions = AgentDefinitionsState::default();
    let mut screen = AppScreen::Launchpad;

    let opened = enter_workspace_for_path(
        &mut runtime,
        WorkspaceCanvasState {
            motion: &mut motion,
            master_layout: &mut master_layout,
        },
        &mut persistence,
        &mut agent_definitions,
        &mut screen,
        &mut workspace_state,
        &workspace_path,
    );

    assert!(
        opened.is_ok(),
        "enter_workspace_for_path 应成功打开已持久化的 workspace，实际: {:?}",
        opened
    );
    let opened = opened.expect("opened workspace result should exist after is_ok assertion");
    assert_eq!(
        screen,
        AppScreen::Canvas,
        "成功打开 workspace 后应切换到 Canvas"
    );
    assert_eq!(
        runtime.live_len(),
        0,
        "打开 workspace 不应同步恢复 terminal live node，否则选择项目会被 PTY spawn 卡住"
    );
    assert_eq!(
        opened.pending_live_restore.len(),
        1,
        "pending restore 应包含保存的 terminal node"
    );
    assert!(
        runtime.restore_live_session_for_node_with_node_sessions(
            6,
            &opened.pending_live_restore.checkpoints,
            &opened.pending_live_restore.node_sessions,
        ),
        "deferred restore 执行 node_id=6 时应成功恢复 terminal live node"
    );
    let view = runtime.node_view(6);
    let Some(NodeView::Term { snapshot, .. }) = view else {
        runtime.kill_all();
        panic!("node_view(6) 不应为 None，否则终端卡片会显示 session gone");
    };
    assert_eq!(
        snapshot.cols, 88,
        "恢复的 live terminal 应使用 checkpoint 中的 88 列"
    );
    assert_eq!(
        snapshot.lines, 26,
        "恢复的 live terminal 应使用 checkpoint 中的 26 行"
    );
    assert!(
        runtime.node_view(6).is_some(),
        "保存过 checkpoint 的 terminal card 不应进入 session gone 状态"
    );
    assert!(
        runtime.node_view(7).is_none(),
        "恢复 node_id=6 不应额外创建 node_id=7 的 live node"
    );
    runtime.kill_all();
}

/// 打开已有 workspace：恢复的 Terminal、Codex 与 Claude Code 卡片应立即登记各自初始化覆盖层。
///
/// 数据构造（含关键数值的推导过程）：
///   canvas snapshot = 3 张可运行卡片：Terminal(id=6)、Codex(id=7)、Claude(id=8)。
///   三张卡片均为 640×360px；没有 checkpoint，因此本测试只验证首帧视觉状态，
///   不依赖异步 PTY 恢复。初始化首帧 index=0，overlay=1.0，content=0.0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入包含 Terminal、Codex 与 Claude Code 的 canvas snapshot。
///   2. 调用 enter_workspace_for_path()，它读取 snapshot 并重建 runtime/document。
///   3. 从 MotionStore 读取三个 node 的初始化 presentation 和对应资源名。
///
/// 预期结果：
///   - 断言 A：Terminal 使用 ghostty-terminal-16-47，保留既有终端开场画面。
///   - 断言 B：Codex 使用 codex-brand-ascii，不能回退到通用 Norma ASCII。
///   - 断言 C：Claude Code 使用 claude-code-brand-ascii，不能错误使用 Codex 资源。
///   - 断言 D：三个首帧均为 overlay=1.0/content=0.0，恢复首屏不应提前透出 PTY 内容。
#[test]
fn test_enter_workspace_starts_profile_specific_intro_for_restored_live_cards() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_path = temp.path().join("restored-brand-intros");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let project_store =
        ProjectStore::for_workspace(&workspace_path).expect("project store should open");
    let snapshot = DocumentSnapshot {
        nodes: vec![
            CanvasNodeSnapshot {
                id: 6,
                kind: CanvasNodeKind::Term(TermSpec),
                bounds: CanvasRect::new(10.0, 20.0, 640.0, 360.0),
                z_index: 1,
            },
            CanvasNodeSnapshot {
                id: 7,
                kind: CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Codex,
                    alias: None,
                    definition_id: None,
                }),
                bounds: CanvasRect::new(20.0, 30.0, 640.0, 360.0),
                z_index: 2,
            },
            CanvasNodeSnapshot {
                id: 8,
                kind: CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Claude,
                    alias: None,
                    definition_id: None,
                }),
                bounds: CanvasRect::new(30.0, 40.0, 640.0, 360.0),
                z_index: 3,
            },
        ],
        viewport: Viewport {
            pan: Vec2::new(0.0, 0.0),
            zoom: 1.0,
        },
    };
    project_store
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("canvas snapshot 应写入 project store");
    let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(false);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut agent_definitions = AgentDefinitionsState::default();
    let mut screen = AppScreen::Launchpad;

    enter_workspace_for_path(
        &mut runtime,
        WorkspaceCanvasState {
            motion: &mut motion,
            master_layout: &mut master_layout,
        },
        &mut persistence,
        &mut agent_definitions,
        &mut screen,
        &mut workspace_state,
        &workspace_path,
    )
    .expect("打开带品牌卡片的 workspace 应成功");

    let terminal = motion
        .initialization_presentation(6)
        .expect("恢复的 Terminal 应在首帧登记初始化覆盖层");
    let codex = motion
        .initialization_presentation(7)
        .expect("恢复的 Codex 应在首帧登记初始化覆盖层");
    let claude = motion
        .initialization_presentation(8)
        .expect("恢复的 Claude Code 应在首帧登记初始化覆盖层");

    assert_eq!(
        crate::cards::intro::intro_asset_name(terminal.kind),
        "ghostty-terminal-16-47",
        "恢复的 Terminal 必须保留 Ghostty 初始化资源"
    );
    assert_eq!(
        crate::cards::intro::intro_asset_name(codex.kind),
        "codex-brand-ascii",
        "恢复的 Codex 必须使用 Codex 品牌 ASCII 资源，而非通用 Norma ASCII"
    );
    assert_eq!(
        crate::cards::intro::intro_asset_name(claude.kind),
        "claude-code-brand-ascii",
        "恢复的 Claude Code 必须使用 Claude Code 品牌 ASCII 资源，而非 Codex 图标"
    );
    assert_eq!(
        terminal.frame_index, 0,
        "恢复的 Terminal 初始化应从第 0 帧开始，不能跳过开场"
    );
    assert_eq!(
        codex.frame_index, 0,
        "恢复的 Codex 初始化应从第 0 帧开始，不能沿用旧动画帧"
    );
    assert_eq!(
        claude.frame_index, 0,
        "恢复的 Claude Code 初始化应从第 0 帧开始，不能沿用 Codex 状态"
    );
    assert_eq!(
        terminal.overlay_opacity, 1.0,
        "恢复首帧的 Terminal 覆盖层必须完全不透明"
    );
    assert_eq!(
        codex.overlay_opacity, 1.0,
        "恢复首帧的 Codex 覆盖层必须完全不透明"
    );
    assert_eq!(
        claude.overlay_opacity, 1.0,
        "恢复首帧的 Claude Code 覆盖层必须完全不透明"
    );
    assert_eq!(
        terminal.content_opacity, 0.0,
        "恢复首帧的 Terminal 不应提前显示 PTY 内容"
    );
    assert_eq!(
        codex.content_opacity, 0.0,
        "恢复首帧的 Codex 不应提前显示 PTY 内容"
    );
    assert_eq!(
        claude.content_opacity, 0.0,
        "恢复首帧的 Claude Code 不应提前显示 PTY 内容"
    );
    assert_ne!(
        crate::cards::intro::intro_asset_name(codex.kind),
        "norma-agent-duotone",
        "Codex 恢复路径不应回退到旧的通用 Norma ASCII 资源"
    );
    assert_ne!(
        crate::cards::intro::intro_asset_name(claude.kind),
        "codex-brand-ascii",
        "Claude Code 恢复路径不应错误显示 Codex 品牌 ASCII 图标"
    );
}

/// 打开已有 workspace：app 层应恢复保存的 master/detail layout state。
///
/// 数据构造（含关键数值的推导过程）：
///   canvas snapshot = Rect A(id=1,bounds 10/20/100/80), Rect B(id=2,bounds 140/20/100/80)
///   layout_state = MasterDetail(master_id=2)，original_bounds 记录 A/B 进入 master 前 bounds
///   初始 master_layout inactive，打开后应 active 且 master_id=2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ProjectStore 写入 canvas snapshot 与 layout_state
///   2. enter_workspace_for_path(workspace)
///      → load_canvas_snapshot 恢复 document
///      → load_state + restore_from_snapshot 恢复 MasterLayoutState
///   3. 查询 master_layout.master_id()
///
/// 预期结果：
///   - 正断言：打开成功，screen == Canvas，master_layout active master 为 2
///   - 正断言：runtime document 仍包含 A/B
///   - 负断言：master_layout 不应被 workspace open 末尾 clear 掉
#[test]
fn test_enter_workspace_restores_saved_master_layout_state() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_path = temp.path().join("restored-master-layout");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let project_store =
        ProjectStore::for_workspace(&workspace_path).expect("project store should open");
    let a_bounds = CanvasRect::new(10.0, 20.0, 100.0, 80.0);
    let b_bounds = CanvasRect::new(140.0, 20.0, 100.0, 80.0);
    let snapshot = DocumentSnapshot {
        nodes: vec![
            CanvasNodeSnapshot {
                id: 1,
                kind: CanvasNodeKind::Rect(canvas::RectNode { color: 0x4080ff }),
                bounds: a_bounds,
                z_index: 1,
            },
            CanvasNodeSnapshot {
                id: 2,
                kind: CanvasNodeKind::Rect(canvas::RectNode { color: 0xff8040 }),
                bounds: b_bounds,
                z_index: 2,
            },
        ],
        viewport: Viewport {
            pan: Vec2::new(0.0, 0.0),
            zoom: 1.0,
        },
    };
    project_store
        .canvas_store()
        .save_snapshot(&snapshot)
        .expect("canvas snapshot 应写入 project store");
    project_store
        .layout_store()
        .save_state(&LayoutStateSnapshot {
            mode: LayoutMode::MasterDetail,
            master_id: Some(2),
            original_bounds: vec![
                LayoutOriginalBounds {
                    node_id: 1,
                    bounds: a_bounds,
                },
                LayoutOriginalBounds {
                    node_id: 2,
                    bounds: b_bounds,
                },
            ],
        })
        .expect("layout state 应写入 project store");
    let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut agent_definitions = AgentDefinitionsState::default();
    let mut screen = AppScreen::Launchpad;

    let result = enter_workspace_for_path(
        &mut runtime,
        WorkspaceCanvasState {
            motion: &mut motion,
            master_layout: &mut master_layout,
        },
        &mut persistence,
        &mut agent_definitions,
        &mut screen,
        &mut workspace_state,
        &workspace_path,
    );

    assert!(
        result.is_ok(),
        "enter_workspace_for_path 应成功打开带 layout_state 的 workspace，实际: {:?}",
        result
    );
    assert_eq!(
        screen,
        AppScreen::Canvas,
        "成功打开 workspace 后应切换到 Canvas"
    );
    assert_eq!(
        master_layout.master_id(),
        Some(2),
        "打开后应恢复 active master_id=2"
    );
    assert!(
        master_layout.is_active(),
        "打开后 master_layout 不应被 clear，应保持 active master/detail"
    );
    assert!(
        runtime.document.node(1).is_some(),
        "恢复 layout state 不应丢失 Rect A"
    );
    assert!(
        runtime.document.node(2).is_some(),
        "恢复 layout state 不应丢失 Rect B"
    );
    assert_ne!(
        master_layout.master_id(),
        None,
        "workspace open 末尾不应无条件清空 master_layout"
    );
}
