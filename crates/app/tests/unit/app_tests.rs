use super::*;
use crate::workspace::flow::{
    enter_workspace_for_path, WorkspaceCanvasState, WorkspaceOpenContext,
};
use crate::workspace::persistence::PersistenceScheduler;
use canvas::{compute_configurable_grid_layout, AgentProfile, AgentSpec, CanvasNodeKind, RectNode};
use runtime::{AgentTaskState, AgentTaskUpdate, CardAttentionKind, CardAttentionState};
use storage::{NewAgentDefinition, ProjectStore, RecentStore};

/// 壁纸主题隔离：原图完整绘制，主题切换不得追加任何环境色覆盖层。
///
/// 数据构造（含关键数值的推导过程）：
///   image opacity = 1.0（先保留完整壁纸像素）
///   wallpaper layer = 1 层
///   theme overlay layers = 0 层
///   overlay angle = 135°（旧实现；必须不存在）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 渲染壁纸图片 → 使用 WALLPAPER_IMAGE_OPACITY=1.0。
///   2. 读取 `render_workspace_backdrop` 生产源码。
///   3. 确认背景挂载壁纸层，但不读取 canvas overlay token 或构造旧 135° 遮罩。
///
/// 预期结果：
///   - 正断言：图片 opacity 为 1.0。
///   - 正断言：工作台背景仍挂载真实壁纸层。
///   - 负断言：不得读取 canvas overlay 起止色。
///   - 负断言：不得追加旧 135° 全屏渐变遮罩。
#[test]
fn test_wallpaper_renders_fully_opaque_without_theme_overlay() {
    let chrome = include_str!("../../src/app/chrome.rs");
    let backdrop = chrome
        .split("pub(crate) fn render_wallpaper_layer")
        .next()
        .unwrap_or(chrome);

    assert_eq!(
        chrome::WALLPAPER_IMAGE_OPACITY,
        1.0,
        "wallpaper image should render fully opaque to preserve original brightness"
    );
    assert!(
        backdrop.contains("render_wallpaper_layer()"),
        "工作台背景必须继续挂载真实壁纸层"
    );
    assert!(
        !backdrop.contains("theme.canvas_overlay_from")
            && !backdrop.contains("theme.canvas_overlay_to"),
        "主题切换不得读取壁纸 overlay token"
    );
    assert!(
        !backdrop.contains("linear_gradient(\n                    135.0"),
        "工作台背景不得追加旧 135° 全屏主题渐变遮罩"
    );
}

/// 运行态光标动画应与 Canvas ambient 共用不超过 15fps 的 repaint cadence。
///
/// 数据构造（含关键数值的推导过程）：
///   target fps = 15fps → repaint 预算 = ceil(1000ms / 15fps) = 67ms
///   app frame loop tick = 12ms
///   前 5 tick 累计 12/24/36/48/60ms < 67ms；第 6 tick 累计 72ms → repaint
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 断言 cursor 使用共享 67ms ambient 常量，而不是旧 33ms 常量。
///   2. 创建默认 FrameThrottle，连续输入六个 12ms tick。
///   3. 逐一读取每一步返回的 repaint delta。
///
/// 预期结果：
///   - 正断言：cursor repaint interval 精确为 67ms。
///   - 负断言：前五个 12ms tick 都不得提前触发 repaint。
///   - 正断言：第六个 tick 返回累计 72ms，供 phase 一次性追上真实时间。
///   - 负断言：放行时不得只返回 12ms，否则动画相位会变慢。
#[test]
fn test_frame_throttle_batches_cursor_repaint_to_at_most_15fps() {
    let mut throttle = FrameThrottle::default();
    let first = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    let second = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    let third = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    let fourth = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    let fifth = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    let sixth = throttle.tick(Duration::from_millis(12), CURSOR_REPAINT_INTERVAL);
    assert_eq!(
        CURSOR_REPAINT_INTERVAL,
        Duration::from_millis(67),
        "cursor 必须引用 ceil(1000ms / 15fps) = 67ms 的共享 ambient cadence"
    );
    assert!(
        first.is_none(),
        "第 1 个 12ms tick 不应触发 focused cursor repaint"
    );
    assert!(
        second.is_none(),
        "第 2 个 12ms tick 累计 24ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        third.is_none(),
        "第 3 个 12ms tick 累计 36ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        fourth.is_none(),
        "第 4 个 12ms tick 累计 48ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        fifth.is_none(),
        "第 5 个 12ms tick 累计 60ms < 67ms，仍不应触发 repaint"
    );
    assert_eq!(
        sixth,
        Some(Duration::from_millis(72)),
        "第 6 个 tick 应返回累计 72ms，让 cursor phase 追上真实时间"
    );
    assert_ne!(
        sixth,
        Some(Duration::from_millis(12)),
        "放行 repaint 时不应只推进 12ms，否则节流会让 cursor 动画变慢"
    );
}
/// 卡片查看 helper：用户选择/聚焦卡片时应把未查看完成提醒标记为已查看。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1，profile=Codex
///   AgentTaskState::Done → runtime 创建 AgentTaskDone attention(viewed=false)
///   mark_runtime_node_viewed_if_needed(node_id) 模拟鼠标点击或方向键选中该卡片
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime document 加入 Codex Agent node
///   2. apply_agent_task_update(Done) 创建未查看 attention
///   3. mark_runtime_node_viewed_if_needed(node_id)
///   4. 再次 mark_runtime_node_viewed_if_needed(node_id)
///
/// 预期结果：
///   - 正断言：首次 helper 返回 true，并把 viewed 改为 true
///   - 正断言：attention kind 保持 AgentTaskDone
///   - 负断言：重复 helper 返回 false，不能重复确认同一提醒
#[test]
fn test_mark_runtime_node_viewed_if_needed_marks_done_attention_viewed() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let changed = runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: "Stop".to_string(),
        to: AgentTaskState::Done,
        task_title: None,
        session_id: None,
    });
    assert!(changed, "Done update 应创建未查看完成提醒");

    let first = mark_runtime_node_viewed_if_needed(&mut runtime, node_id);
    let attention = runtime.card_attention(node_id);
    let second = mark_runtime_node_viewed_if_needed(&mut runtime, node_id);

    assert!(first, "首次查看 Done attention 应返回 true");
    assert_eq!(
        attention,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: true,
        }),
        "查看后 attention 应保留 AgentTaskDone kind，并标记 viewed=true"
    );
    assert!(
        !second,
        "重复调用 helper 不应再次返回 true，否则 UI 会重复处理同一提醒"
    );
    assert_ne!(
        attention,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: false,
        }),
        "查看后不应仍保持 viewed=false，否则呼吸灯不会关闭"
    );
}

/// 删除中间卡片后，剩余卡片应立即按默认的每行三张布局策略重排。
///
/// 数据构造（含关键数值的推导过程）：
///   area = origin(0,0), size(1200x900)
///   zoom = 1.0
///   默认策略 = 按行向下 + 每行 3 张 + 适应可视区域
///   usable width = 1200 - 96 - 32 = 1072；usable height = 900 - 86 - 84 = 730
///   gap = 8；三节点布局 rows = ceil(3/3) = 1，cols = 3
///   card_width = (1072 - 8*(3+1)) / 3 = 1040/3；card_height = 730 - 8*(1+1) = 714
///   C 位于第三列：x = 96 + 8 + 2*(1040/3+8)，y = 86 + 8 = 94
///   删除中间节点后 rows = ceil(2/3) = 1，cols = 3，尺寸不变
///   C 移到第二列：x = 96 + 8 + (1040/3+8)，y = 94
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A/B/C 三张普通卡片，并手动放到默认的一行三列布局位置
///   2. close_node_and_reflow_for_area(B) → 删除 B，并用剩余顺序 [A,C] 重新计算布局
///   3. 查询 A/B/C 的 document bounds
///
/// 预期结果：
///   - 正断言：B 被删除，返回 true
///   - 正断言：A 仍位于默认布局的第一列第一行，尺寸不变
///   - 正断言：C 移到默认布局的第二列第一行
///   - 负断言：C 不应停留在删除前的第三列位置
#[test]
fn test_close_node_reflows_remaining_cards_with_default_layout_strategy() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 900.0);
    let zoom = 1.0;

    let a = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(0.0, 0.0, 10.0, 10.0),
    );
    let b = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
        CanvasRect::new(0.0, 0.0, 10.0, 10.0),
    );
    let c = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x35d07f }),
        CanvasRect::new(0.0, 0.0, 10.0, 10.0),
    );

    let default_strategy = CanvasLayoutStrategy::default();
    let three_nodes_layout = compute_configurable_grid_layout(
        &[a, b, c],
        area,
        default_strategy.grid_config_for_zoom(zoom),
    );
    runtime
        .document
        .set_node_bounds(a, three_nodes_layout[0].bounds);
    runtime
        .document
        .set_node_bounds(b, three_nodes_layout[1].bounds);
    runtime
        .document
        .set_node_bounds(c, three_nodes_layout[2].bounds);
    let c_right_column_bounds = three_nodes_layout[2].bounds;

    let removed = close_node_and_reflow_for_area(&mut runtime, &mut motion, b, area, zoom);

    let two_nodes_layout = compute_configurable_grid_layout(
        &[a, c],
        area,
        default_strategy.grid_config_for_zoom(zoom),
    );
    let a_after = runtime
        .document
        .node(a)
        .expect("A 应在删除 B 后继续存在")
        .bounds;
    let c_after = runtime
        .document
        .node(c)
        .expect("C 应在删除 B 后继续存在")
        .bounds;

    assert!(
        removed,
        "close_node_and_reflow_for_area(B) 应返回 true，表示确实删除了中间卡片"
    );
    assert!(
        runtime.document.node(b).is_none(),
        "B 应从 document 删除，不能继续参与渲染或布局"
    );
    assert_eq!(
        a_after, two_nodes_layout[0].bounds,
        "A 应按删除后的默认布局策略占据第一列第一行，并保留尺寸"
    );
    assert_eq!(
        c_after, two_nodes_layout[1].bounds,
        "C 应按删除后的默认布局策略移动到第二列第一行"
    );
    assert_ne!(
        c_after, c_right_column_bounds,
        "C 不应停留在删除前的第三列位置，否则说明删除没有触发重排"
    );
}

/// 打开已有 workspace：app 层应记录最近项目、切换到 Canvas，并把 agent state 绑定到该项目的本地定义库。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace path = temp/launchpad-opened，真实目录，因此 path.is_dir() = true
///   project-local agent defs = 1 条，name = "Workspace"
///   storage path = temp/state/app.sqlite
///   初始 screen = Launchpad，runtime.workspace_root = None，recent count = 0，agent_definitions 未绑定
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先为 workspace 创建并 migrate ProjectStore，写入 1 条项目本地 agent definition
///   2. 构造 Runtime::new()、MotionStore::new(true)、AppScreen::Launchpad、AgentDefinitionsState::default()
///   3. WorkspaceState::load_from_storage(storage)
///   4. enter_workspace_for_path(workspace)
///      → WorkspaceState::open_workspace 写 SQLite
///      → runtime 替换为 Runtime::new_for_workspace(root)
///      → agent_definitions.bind_project_store(project_store.agent_definition_store())
///      → screen = Canvas
///   5. 查询 screen/runtime/workspace_state/agent_definitions
///
/// 预期结果：
///   - 正断言：返回 Ok 且 screen == Canvas
///   - 正断言：runtime.workspace_root/current_root/recent[0].path 都是 canonical workspace
///   - 正断言：agent_definitions 只加载该 workspace 的本地 agent definition
///   - 负断言：screen 不应仍为 Launchpad，recent 不应为空，agent_definitions 不应保持未绑定
#[test]
fn test_enter_workspace_records_recent_and_switches_to_canvas() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let database_paths = storage::DatabasePaths::production_for_home(temp.path());
    let workspace_path = temp.path().join("launchpad-opened");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let canonical_workspace = workspace_path
        .canonicalize()
        .expect("workspace should canonicalize");
    let project_store = ProjectStore::for_database_paths(&database_paths, &workspace_path)
        .expect("project store should open for workspace-local agent setup");
    project_store
        .migrate()
        .expect("project store should migrate before agent definition write");
    project_store
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "Workspace".to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "Workspace-local workflow".to_string(),
        })
        .expect("workspace-local agent definition should persist");
    let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut agent_definitions = AgentDefinitionsState::default();
    let mut screen = AppScreen::Launchpad;

    let result = enter_workspace_for_path(
        WorkspaceOpenContext {
            database_paths: &database_paths,
            runtime: &mut runtime,
            canvas: WorkspaceCanvasState {
                motion: &mut motion,
                master_layout: &mut master_layout,
            },
            persistence: &mut persistence,
            agent_definitions: &mut agent_definitions,
            screen: &mut screen,
            workspace_state: &mut workspace_state,
        },
        &workspace_path,
    );

    assert!(
        result.is_ok(),
        "enter_workspace_for_path 应成功打开真实目录，实际: {:?}",
        result
    );
    assert_eq!(
        screen,
        AppScreen::Canvas,
        "成功打开 workspace 后应从 Launchpad 切换到 Canvas"
    );
    assert_eq!(
        runtime.workspace_root(),
        Some(canonical_workspace.as_path()),
        "进入画布后 Runtime 应持有 workspace cwd"
    );
    assert_eq!(
        workspace_state.current_root.as_deref(),
        Some(canonical_workspace.as_path()),
        "WorkspaceState.current_root 应记录当前 workspace"
    );
    assert_eq!(
        workspace_state.recent.records[0].path, canonical_workspace,
        "最近项目第一条应是刚打开的 workspace"
    );
    assert_eq!(
        agent_definitions.definitions().len(),
        1,
        "打开 workspace 后 agent state 应加载该项目本地的 1 条 agent 定义"
    );
    assert_eq!(
        agent_definitions.definitions()[0].name,
        "Workspace",
        "agent state 应绑定到当前 workspace 的 project store，而不是保持空状态"
    );
    assert_ne!(
        screen,
        AppScreen::Launchpad,
        "成功打开 workspace 后不应停留在 Launchpad"
    );
    assert!(
        !workspace_state.recent.records.is_empty(),
        "成功打开 workspace 后 recent 不应为空"
    );
    assert!(
        !agent_definitions.voice_context().is_empty(),
        "打开 workspace 后 agent_definitions 不应保持未绑定，否则 voice context 仍为空"
    );
}

/// 场景：打开不存在的 workspace 路径时，app 应返回错误并保持 Launchpad 界面不切换到 Canvas。
///
/// 数据构造（含关键数值的推导过程）：
///   missing path = temp/missing-recent，未创建，因此 path.is_dir() = false
///   初始 screen = Launchpad
///   初始 runtime.workspace_root = None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造空 runtime/motion/workspace_state
///   2. enter_workspace_for_path(missing)
///   3. WorkspaceState::open_workspace 发现目录不存在并返回 Err
///   4. 查询 screen/runtime/error/recent
///
/// 预期结果：
///   - 正断言：返回 Err，screen 仍为 Launchpad
///   - 正断言：error 包含 missing-recent
///   - 负断言：runtime.workspace_root 不应被设置，recent 不应新增记录
#[test]
fn test_enter_missing_workspace_stays_on_launchpad_with_error() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let database_paths = storage::DatabasePaths::production_for_home(temp.path());
    let missing_path = temp.path().join("missing-recent");
    let storage = RecentStore::new(temp.path().join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut agent_definitions = AgentDefinitionsState::default();
    let mut screen = AppScreen::Launchpad;

    let result = enter_workspace_for_path(
        WorkspaceOpenContext {
            database_paths: &database_paths,
            runtime: &mut runtime,
            canvas: WorkspaceCanvasState {
                motion: &mut motion,
                master_layout: &mut master_layout,
            },
            persistence: &mut persistence,
            agent_definitions: &mut agent_definitions,
            screen: &mut screen,
            workspace_state: &mut workspace_state,
        },
        &missing_path,
    );

    assert!(
        result.is_err(),
        "打开不存在 workspace 应返回 Err，让 UI 保持在 Launchpad"
    );
    assert_eq!(
        screen,
        AppScreen::Launchpad,
        "打开失败后应继续停留在 Launchpad"
    );
    assert!(
        workspace_state
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("missing-recent"),
        "错误状态应说明哪个最近项目路径不存在，实际: {:?}",
        workspace_state.error
    );
    assert!(
        runtime.workspace_root().is_none(),
        "打开失败不应设置 runtime.workspace_root"
    );
    assert!(
        workspace_state.recent.records.is_empty(),
        "打开失败不应向最近项目写入记录"
    );
}
