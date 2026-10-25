use super::geometry::{ring_distance, CardFrame};
use super::*;
use runtime::{AgentTaskState, CardAttentionKind, CardAttentionState};
use ui::Theme;

fn palette() -> RunPalette {
    RunPalette::from_theme(Theme::default())
}

/// 620×420 的卡片放在 56px 留白里，圆角 14，标题栏 32，缩放 1。
fn frame() -> CardFrame {
    CardFrame {
        x: 56.0,
        y: 56.0,
        w: 620.0,
        h: 420.0,
        radius: 14.0,
        zoom: 1.0,
        title_h: 32.0,
    }
}

fn input(state: RunState) -> RunInput {
    RunInput {
        state,
        viewed: false,
        tool_key: 0,
    }
}

/// 按 16ms 一帧推进 `ms` 毫秒。
fn run(motion: &mut RunMotion, style: RunStyle, ms: f32) {
    let mut left = ms;
    while left > 0.0 {
        let dt = left.min(16.0);
        motion.tick(dt, style, &palette(), false);
        left -= dt;
    }
}

/// 风格 id 往返；未知值和空值回落到经典。
///
/// 数据：四个风格的 id，以及 "aurora"（不存在）和 None。
/// 预期：from_id(id(s)) == s；"aurora"/None → Classic，不 panic。
#[test]
fn run_style_id_round_trips_and_unknown_falls_back_to_classic() {
    for style in RunStyle::ALL {
        assert_eq!(
            RunStyle::from_id(Some(style.id())),
            style,
            "{} 应能往返",
            style.id()
        );
    }
    assert_eq!(
        RunStyle::from_id(Some("aurora")),
        RunStyle::Classic,
        "未知风格回落经典"
    );
    assert_eq!(RunStyle::from_id(None), RunStyle::Classic, "未设置时为经典");
    assert_eq!(RunStyle::default(), RunStyle::Classic, "默认风格保持现状");
}

/// AgentTaskState + 注意力 → RunInput 的映射表。
///
/// 数据：七种任务状态；完成分“未查看”和“已查看/无注意力”。
/// 预期：Ended/None 视为准备；完成未查看时 viewed=false；工具名不同指纹不同。
#[test]
fn run_input_maps_task_states_and_done_attention() {
    let unviewed = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: false,
    });
    let cases = [
        (Some(AgentTaskState::Ready), RunState::Ready),
        (Some(AgentTaskState::Ended), RunState::Ready),
        (None, RunState::Ready),
        (Some(AgentTaskState::Thinking), RunState::Thinking),
        (
            Some(AgentTaskState::WaitingApproval {
                tool_name: Some("Bash".into()),
            }),
            RunState::Approval,
        ),
        (
            Some(AgentTaskState::Failed {
                message: "boom".into(),
            }),
            RunState::Failed,
        ),
    ];
    for (task, expected) in cases {
        assert_eq!(
            run_input(task.as_ref(), None).state,
            expected,
            "{task:?} 应映射为 {expected:?}"
        );
    }
    let done = run_input(Some(&AgentTaskState::Done), unviewed);
    assert_eq!(done.state, RunState::Done, "完成映射为 Done");
    assert!(!done.viewed, "未查看的完成提示 viewed=false");
    assert!(
        run_input(Some(&AgentTaskState::Done), None).viewed,
        "没有注意力记录视为已查看"
    );
    let read = run_input(
        Some(&AgentTaskState::RunningTool {
            tool_name: Some("read_file".into()),
        }),
        None,
    );
    let edit = run_input(
        Some(&AgentTaskState::RunningTool {
            tool_name: Some("edit_file".into()),
        }),
        None,
    );
    assert_eq!(read.state, RunState::Tool, "运行工具映射为 Tool");
    assert_ne!(
        read.tool_key, edit.tool_key,
        "不同工具名的指纹必须不同，才能识别切换"
    );
}

/// 思考到一半切到等待审批：颜色从当前值出发，不先跳回准备色。
///
/// 数据：灵光；思考 300ms（颜色接近蓝），再切 Approval 推进 16ms。
/// 执行：记录切换前颜色 c0，切换后一帧颜色 c1，目标暖色 cw。
/// 预期：c1 与 c0 的距离小于 c1 与准备灰的距离（连续）；且 c1 朝 cw 移动。
#[test]
fn state_switch_continues_from_current_color() {
    let p = palette();
    let mut motion = RunMotion::new(RunStyle::Aura, input(RunState::Thinking), &p);
    run(&mut motion, RunStyle::Aura, 300.0);
    let before = motion.color;
    motion.set_input(input(RunState::Approval));
    motion.tick(16.0, RunStyle::Aura, &p, false);
    let after = motion.color;
    let dist = |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<f32>();
    let ready = p.state_color(RunState::Ready);
    let warm = p.state_color(RunState::Approval);
    assert!(
        dist(after, before) < dist(after, ready),
        "切换后第一帧应贴着切换前的颜色，而不是回到准备色"
    );
    assert!(dist(after, warm) < dist(before, warm), "颜色应朝暖色移动");
}

/// 工具切换反馈节流：600ms 内第二次切换不触发；同名工具不触发。
///
/// 数据：进入 Tool(key=1) → 200ms 后切 key=2 → 再 500ms（累计 700ms）切 key=3。
/// 预期：进入时 since_ping=0；200ms 处切换后 since_ping≈200（被节流）；
///       700ms 处切换后 since_ping=0；重复 key=3 不改变 since_ping。
#[test]
fn tool_ping_is_throttled_and_ignores_same_tool() {
    let p = palette();
    let mut motion = RunMotion::new(RunStyle::Underline, input(RunState::Thinking), &p);
    let tool = |key| RunInput {
        state: RunState::Tool,
        viewed: false,
        tool_key: key,
    };
    motion.set_input(tool(1));
    assert_eq!(motion.since_ping, 0.0, "进入工具调用触发一次 ping");
    run(&mut motion, RunStyle::Underline, 200.0);
    motion.set_input(tool(2));
    assert!(
        motion.since_ping >= 199.0,
        "200ms 内的切换被 600ms 节流，since_ping 不归零"
    );
    run(&mut motion, RunStyle::Underline, 500.0);
    motion.set_input(tool(3));
    assert_eq!(
        motion.since_ping, 0.0,
        "距上次 ping 700ms(>600) 的切换应触发"
    );
    run(&mut motion, RunStyle::Underline, 700.0);
    assert!(!motion.set_input(tool(3)), "同名工具不是切换");
    assert!(motion.since_ping >= 699.0, "同名工具不触发 ping");
}

/// 完成后查看：强度淡出到 0，之后不再请求帧；未查看时 1.6s 后定格。
///
/// 数据：灵光；思考 → 完成（未查看）推进 2000ms → 标记已查看推进 2000ms。
/// 预期：未查看 2000ms 后 wants_frames=false 但 level≈0.5（提示保留）；
///       已查看后 level<0.01、灵光不再出图元、wants_frames=false。
#[test]
fn done_settles_then_fades_after_viewed() {
    let p = palette();
    let mut motion = RunMotion::new(RunStyle::Aura, input(RunState::Thinking), &p);
    run(&mut motion, RunStyle::Aura, 500.0);
    motion.set_input(input(RunState::Done));
    run(&mut motion, RunStyle::Aura, 2000.0);
    assert!(
        !motion.wants_frames(false),
        "完成 1.6s 后画面定格，不再出帧"
    );
    assert!(
        (motion.level - 0.5).abs() < 0.01,
        "未查看时保留完成提示，level≈0.5"
    );
    assert!(
        !build(RunStyle::Aura, &motion, &frame(), false).is_empty(),
        "未查看时仍画出绿光"
    );
    motion.set_input(RunInput {
        state: RunState::Done,
        viewed: true,
        tool_key: 0,
    });
    assert!(motion.wants_frames(false), "刚查看时需要出帧淡出");
    run(&mut motion, RunStyle::Aura, 2000.0);
    assert!(motion.level < 0.01, "查看后淡出到 0");
    assert!(
        build(RunStyle::Aura, &motion, &frame(), false).is_empty(),
        "查看后不留残留图元"
    );
    assert!(!motion.wants_frames(false), "淡出完成后停帧");
}

/// 准备状态平滑完成后停帧；思考、工具、等待审批持续出帧；减少动态效果不出帧。
///
/// 数据：磁流体从准备开始推进 3000ms；再切思考。
/// 预期：准备稳定后 false；思考 true；reduce_motion=true 时恒 false。
#[test]
fn frame_requests_follow_state_and_reduce_motion() {
    let p = palette();
    let mut motion = RunMotion::new(RunStyle::Ferrofluid, input(RunState::Ready), &p);
    run(&mut motion, RunStyle::Ferrofluid, 3000.0);
    assert!(!motion.wants_frames(false), "准备状态稳定后停帧");
    for state in [RunState::Thinking, RunState::Tool, RunState::Approval] {
        motion.set_input(input(state));
        assert!(motion.wants_frames(false), "{state:?} 持续出帧");
        assert!(
            !motion.wants_frames(true),
            "{state:?} 在减少动态效果下不出帧"
        );
    }
}

/// 一帧间隔极大（窗口挂起 1 小时）时数值不发散。
///
/// 数据：dt = 3_600_000ms。预期：level、颜色、各标量都是有限数，且等于目标附近。
#[test]
fn huge_frame_gap_does_not_diverge() {
    let p = palette();
    for style in RunStyle::ALL {
        let mut motion = RunMotion::new(style, input(RunState::Tool), &p);
        motion.tick(3_600_000.0, style, &p, false);
        assert!(
            motion.level.is_finite() && (motion.level - 1.2).abs() < 0.01,
            "{style:?} level 应到目标 1.2"
        );
        assert!(
            motion.values.iter().all(|v| v.is_finite()),
            "{style:?} 标量有限"
        );
        let prims = build(style, &motion, &frame(), false);
        assert!(
            prims.len() <= MAX_PRIMS,
            "{style:?} 图元数 {} 超上限",
            prims.len()
        );
    }
}

/// 周长几何：起点、底边中点、环形距离。
///
/// 数据：frame()：x=56,y=56,w=620,h=420,r=14。
/// 预期：edge(0) = (x+r, y) 法线朝上；edge(bottom_center_u) = (x+w/2, y+h) 法线朝下；
///       ring_distance(0.95, 0.05) = 0.1（跨 0 环绕）。
#[test]
fn rounded_rect_edge_geometry() {
    let f = frame();
    let start = f.edge(0.0);
    assert!(
        (start.x - 70.0).abs() < 0.01 && (start.y - 56.0).abs() < 0.01,
        "起点在上边左端圆角后: {start:?}"
    );
    assert_eq!((start.nx, start.ny), (0.0, -1.0), "上边法线朝上");
    let bottom = f.edge(f.bottom_center_u());
    assert!(
        (bottom.x - 366.0).abs() < 0.5 && (bottom.y - 476.0).abs() < 0.01,
        "底边中点 = (56+310, 56+420): {bottom:?}"
    );
    assert!(
        (ring_distance(0.95, 0.05) - 0.1).abs() < 1e-5,
        "环形距离跨 0 环绕"
    );
}

#[path = "run_style_render_tests.rs"]
mod render;
