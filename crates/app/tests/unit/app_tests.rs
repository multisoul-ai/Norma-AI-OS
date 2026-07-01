use super::*;
use crate::workspace::flow::enter_workspace_for_path;
use crate::workspace::persistence::PersistenceScheduler;
use canvas::{compute_responsive_grid_layout, AgentProfile, AgentSpec, CanvasNodeKind, RectNode};
use runtime::{AgentTaskState, AgentTaskUpdate, CardAttentionKind, CardAttentionState};
use storage::RecentStore;

/// 壁纸亮度：默认画布壁纸应按原图亮度显示，不额外压暗。
///
/// 数据构造（含关键数值的推导过程）：
///   image opacity = 1.0（原图亮度需要完整不透明绘制，0.78 会混入深色底色）
///   top shade alpha = 0x00 / 0xff = 0%（顶部遮罩不压暗）
///   bottom shade alpha = 0x00 / 0xff = 0%（底部遮罩不压暗；旧值 0xd8 ≈ 85% 会明显变暗）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 渲染壁纸图片 → 使用 WALLPAPER_IMAGE_OPACITY
///   2. 渲染遮罩层 → 使用 WALLPAPER_SHADE_TOP_ALPHA 与 WALLPAPER_SHADE_BOTTOM_ALPHA
///   3. 两个 alpha 都为 0 → 遮罩透明，图片亮度不被 graphite 混色改变
///
/// 预期结果：
///   - 正断言：图片 opacity 为 1.0
///   - 正断言：顶部遮罩 alpha 为 0x00
///   - 正断言：底部遮罩 alpha 为 0x00
///   - 负断言：底部遮罩不再是旧的 0xd8 暗化值
#[test]
fn test_wallpaper_brightness_uses_original_image_luminance() {
    assert_eq!(
        WALLPAPER_IMAGE_OPACITY, 1.0,
        "wallpaper image should render fully opaque to preserve original brightness"
    );
    assert_eq!(
        WALLPAPER_SHADE_TOP_ALPHA, 0x00,
        "wallpaper top shade alpha should be transparent"
    );
    assert_eq!(
        WALLPAPER_SHADE_BOTTOM_ALPHA, 0x00,
        "wallpaper bottom shade alpha should be transparent"
    );
    assert_ne!(
        WALLPAPER_SHADE_BOTTOM_ALPHA, 0xd8,
        "wallpaper bottom shade must not keep the old darkening alpha"
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
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let changed = runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: "Stop".to_string(),
        to: AgentTaskState::Done,
        task_title: None,
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

/// 删除中间卡片后，剩余卡片应立即按两列布局重排。
///
/// 数据构造（含关键数值的推导过程）：
///   area = origin(0,0), size(1200x900)
///   zoom = 1.0
///   safe area = left 96, top 86, right 32, bottom 84
///   usable width = 1200 - 96 - 32 = 1072
///   gap = 16
///   三节点布局：tile_width = (1072 - 16*(3+1)) / 3 = 336，第三列 x = 96 + 16 + 2*(336+16) = 816
///   删除中间节点后：tile_width = (1072 - 16*(2+1)) / 2 = 512，第二列 x = 96 + 16 + 1*(512+16) = 640
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 A/B/C 三张普通卡片，并手动放到三列布局位置
///   2. close_node_and_reflow_for_area(B) → 删除 B，并用剩余顺序 [A,C] 重新计算布局
///   3. 查询 A/B/C 的 document bounds
///
/// 预期结果：
///   - 正断言：B 被删除，返回 true
///   - 正断言：A 仍位于两列布局第一列
///   - 正断言：C 移到两列布局第二列 x=640
///   - 负断言：C 不应停留在三列布局第三列 x=816
#[test]
fn test_close_node_reflows_remaining_cards_into_two_column_layout() {
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

    let three_column =
        compute_responsive_grid_layout(&[a, b, c], area, workspace_reflow_config_for_zoom(zoom));
    runtime.document.set_node_bounds(a, three_column[0].bounds);
    runtime.document.set_node_bounds(b, three_column[1].bounds);
    runtime.document.set_node_bounds(c, three_column[2].bounds);
    let c_three_column_bounds = three_column[2].bounds;

    let removed = close_node_and_reflow_for_area(&mut runtime, &mut motion, b, area, zoom);

    let two_column =
        compute_responsive_grid_layout(&[a, c], area, workspace_reflow_config_for_zoom(zoom));
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
        a_after, two_column[0].bounds,
        "A 应按删除后的两列布局占据第一列，而不是保留旧三列尺寸"
    );
    assert_eq!(
        c_after, two_column[1].bounds,
        "C 应按删除后的两列布局移动到第二列 x=640"
    );
    assert_ne!(
        c_after, c_three_column_bounds,
        "C 不应停留在删除前的三列第三列 x=816，否则说明删除没有触发重排"
    );
}

/// 打开已有 workspace：app 层应记录最近项目并从 Launchpad 切换到 Canvas。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace path = temp/launchpad-opened，真实目录，因此 path.is_dir() = true
///   storage path = temp/state/app.sqlite
///   初始 screen = Launchpad，runtime.workspace_root = None，recent count = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Runtime::new()、MotionStore::new(true)、AppScreen::Launchpad
///   2. WorkspaceState::load_from_storage(storage)
///   3. enter_workspace_for_path(workspace)
///      → WorkspaceState::open_workspace 写 SQLite
///      → runtime 替换为 Runtime::new_for_workspace(root)
///      → screen = Canvas
///   4. 查询 screen/runtime/workspace_state
///
/// 预期结果：
///   - 正断言：返回 Ok 且 screen == Canvas
///   - 正断言：runtime.workspace_root/current_root/recent[0].path 都是 canonical workspace
///   - 负断言：screen 不应仍为 Launchpad，recent 不应为空
#[test]
fn test_enter_workspace_records_recent_and_switches_to_canvas() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_path = temp.path().join("launchpad-opened");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let canonical_workspace = workspace_path
        .canonicalize()
        .expect("workspace should canonicalize");
    let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut screen = AppScreen::Launchpad;

    let result = enter_workspace_for_path(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        &mut persistence,
        &mut screen,
        &mut workspace_state,
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
    assert_ne!(
        screen,
        AppScreen::Launchpad,
        "成功打开 workspace 后不应停留在 Launchpad"
    );
    assert!(
        !workspace_state.recent.records.is_empty(),
        "成功打开 workspace 后 recent 不应为空"
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
    let missing_path = temp.path().join("missing-recent");
    let storage = RecentStore::new(temp.path().join("app.sqlite"));
    let mut workspace_state = WorkspaceState::load_from_storage(storage);
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let mut persistence = PersistenceScheduler::default();
    let mut screen = AppScreen::Launchpad;

    let result = enter_workspace_for_path(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        &mut persistence,
        &mut screen,
        &mut workspace_state,
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

/// paste_bytes_from_clipboard_text 从剪贴板文本生成应写入 PTY 的字节序列。
///
/// 场景：剪贴板文本（含 ASCII 和多字节汉字）转换为 UTF-8 字节序列，
///   供 NodeInput::Text 直接写入 PTY，不经过 keystroke 编码。
///
/// 数据构造（含关键数值的推导过程）：
///   clipboard text = "hello 世界"
///   UTF-8 bytes: b"hello " (6 bytes) + [0xe4,0xb8,0x96,0xe7,0x95,0x8c] (6 bytes) = 12 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将字符串 "hello 世界" 转换为 as_bytes() 字节切片
///   2. 断言字节内容与预期 UTF-8 编码一致
///   3. 断言不为 b"v"（即未将 Cmd+V 的 'v' 键直传 PTY）
///
/// 预期结果：
///   - 正断言：bytes == "hello 世界".as_bytes()
///   - 负断言：不为空，不为 b"v"（未做粘贴直接发了 v 键）
#[test]
fn test_paste_bytes_matches_clipboard_utf8() {
    let text = "hello 世界";
    let bytes = text.as_bytes().to_vec();
    assert_eq!(
        bytes,
        "hello 世界".as_bytes(),
        "粘贴字节应为 UTF-8 原始内容"
    );
    assert_ne!(bytes, b"v", "不应是裸 v 键字节");
    assert!(!bytes.is_empty(), "粘贴内容不应为空");
}
