use super::*;
use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document, PreviewSpec, PreviewType,
    RectNode, Selection, TermSpec, Vec2, Viewport,
};

#[path = "card_scroll_wheel_tests.rs"]
mod wheel_tests;

fn preview_kind() -> CanvasNodeKind {
    CanvasNodeKind::Preview(PreviewSpec {
        path: "/tmp/report.md".to_string(),
        explicit_type: Some(PreviewType::Markdown),
    })
}

fn term_kind() -> CanvasNodeKind {
    CanvasNodeKind::Term(TermSpec)
}

fn agent_kind(profile: AgentProfile) -> CanvasNodeKind {
    CanvasNodeKind::Agent(AgentSpec {
        profile,
        alias: None,
    })
}

fn selected_selection(id: NodeId) -> Selection {
    let mut selection = Selection::new();
    selection.select_only(id);
    selection
}

fn screen_point(viewport: Viewport, canvas_x: f32, canvas_y: f32) -> Vec2 {
    viewport.canvas_to_screen(Vec2::new(canvas_x, canvas_y))
}

/// 已选 Preview 正文区命中时，滚轮应归 Preview 内部滚动而不是画布 viewport。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   node_top_screen_y = 50*2 + 10 = 110
///   title_bottom_screen_y = 110 + TITLE_BAR_H(26)*2 = 162
///   正文命中点 canvas=(130,80) → screen=(130*2+20, 80*2+10)=(280,170)
///   170 > 162，因此命中正文区而非标题栏
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. wheel_scroll_owner_for_point 用 screen=(280,170) 做命中归属判断
///
/// 预期结果：
///   - 正断言：归属为 NativeElement(preview)，Preview 的 GPUI overflow_scroll 可消费 wheel
///   - 负断言：不应归属 CanvasViewport，否则滚轮会继续平移画布
#[test]
fn test_selected_preview_body_scroll_belongs_to_native_element() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::NativeElement(preview),
        "已选 Preview 正文区滚轮应归 NativeElement，避免画布抢走 GPUI overflow_scroll"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Preview 正文区滚轮不应归 CanvasViewport，否则会触发画布滚动而不是内容滚动"
    );
}

/// 已选 Preview 标题栏命中时，滚轮仍应归画布而不是 Preview 正文滚动。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   node_top_screen_y = 50*2 + 10 = 110
///   title_bottom_screen_y = 110 + 26*2 = 162
///   标题栏命中点 canvas=(130,60) → screen=(280,130)
///   130 <= 162，因此命中标题栏区域
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. 对标题栏 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 CanvasViewport，标题栏保留画布/拖拽语义
///   - 负断言：不应归属 NativeElement(preview)，否则标题栏也会吞掉画布滚轮
#[test]
fn test_selected_preview_title_bar_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 130.0, 60.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Preview 标题栏滚轮应归 Canvas，标题栏不属于内部滚动内容区"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::NativeElement(preview),
        "已选 Preview 标题栏不应归 Preview card，否则标题栏会错误吞掉画布滚轮"
    );
}

/// 未选 Preview 正文区命中时，滚轮应保持画布 viewport 语义。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文命中点 canvas=(130,80) → screen=(280,170)，位于标题栏下方
///   selection 为空，因此 Preview 未被用户选中
///
/// 执行过程：
///   1. add Preview 节点但不 select
///   2. 对正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 CanvasViewport，未选卡片不抢全局滚轮
///   - 负断言：不应归属 NativeElement(preview)，否则 hover 到未选卡片也会改变滚轮语义
#[test]
fn test_unselected_preview_body_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = Selection::new();
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "未选 Preview 正文区滚轮应继续归 Canvas，只有选中态才切换到内部滚动"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::NativeElement(preview),
        "未选 Preview 不应接管滚轮，否则用户路过卡片时会失去画布 zoom"
    );
}

/// 已选非 Preview 卡片正文区命中时，滚轮应仍归画布。
///
/// 数据构造（含关键数值推导）：
///   rect bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文命中点 canvas=(130,80) → screen=(280,170)
///   Rect card 的 wheel_scroll_mode 默认为 CanvasViewport
///
/// 执行过程：
///   1. add Rect 节点并 select_only(rect)
///   2. 对正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 CanvasViewport，非滚动卡片不改变画布滚轮
///   - 负断言：不应归属 TerminalScrollback(rect)，否则普通卡片会吞掉画布滚动
#[test]
fn test_selected_non_preview_body_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let rect = doc.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(100.0, 50.0, 300.0, 220.0),
    );
    let selection = selected_selection(rect);
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Rect 正文区滚轮应归 Canvas，因为 Rect card 未声明内部滚动"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::TerminalScrollback(rect),
        "已选 Rect 不应接管滚轮，否则非滚动卡片会阻断画布滚动"
    );
}

/// 已选 Preview 但指针在卡片外时，滚轮应继续归画布 viewport。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   外部点 canvas=(10,10) → screen=(40,30)
///   preview 覆盖 x∈[100,400), y∈[50,270)，canvas=(10,10) 不在范围内
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. 对卡片外 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 CanvasViewport，卡片外保持全局画布滚轮
///   - 负断言：不应归属 NativeElement(preview)，否则选中后会全局吞滚轮
#[test]
fn test_selected_preview_outside_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 10.0, 10.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Preview 外部滚轮应归 Canvas，方便用户继续缩放/导航画布"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::NativeElement(preview),
        "已选 Preview 不应全局接管滚轮，否则必须取消选择才能滚动画布"
    );
}

/// 已选 Terminal 正文区命中时，滚轮应归 terminal scrollback。
///
/// 数据构造（含关键数值推导）：
///   terminal bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   node_top_screen_y = 50*2 + 10 = 110
///   title_bottom_screen_y = 110 + 26*2 = 162
///   正文命中点 canvas=(130,80) → screen=(280,170)，170 > 162
///
/// 执行过程：
///   1. add Term 节点并 select_only(term)
///   2. 对正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 TerminalScrollback(term)，滚轮查看终端历史
///   - 负断言：不应归属 CanvasViewport，否则终端正文滚轮仍会滚动画布
#[test]
fn test_selected_terminal_body_scroll_belongs_to_terminal_scrollback() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let term = doc.add_node(term_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(term);
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::TerminalScrollback(term),
        "已选 Terminal 正文区滚轮应归 terminal scrollback，而不是画布 zoom"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Terminal 正文区滚轮不应归 CanvasViewport，否则会继续和画布滚动冲突"
    );
}

/// 已选 Claude/Codex/OpenCode Agent 正文区命中时，滚轮应同样归 terminal-owned wheel。
///
/// 数据构造（含关键数值推导）：
///   Claude bounds = origin(100,50) size(300x220)
///   Codex bounds  = origin(500,50) size(300x220)
///   OpenCode bounds = origin(900,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文 y=80 → screen_y = 80*2 + 10 = 170
///   title_bottom_screen_y = 50*2 + 10 + 26*2 = 162，因此三个点都在正文区
///
/// 执行过程：
///   1. add Claude/Codex/OpenCode 三个 Agent 节点
///   2. 分别 select_only 并命中各自正文点
///   3. 对三个 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：Claude 归属 TerminalScrollback(claude)
///   - 正断言：Codex 归属 TerminalScrollback(codex)
///   - 正断言：OpenCode 归属 TerminalScrollback(opencode)
///   - 负断言：Claude/Codex/OpenCode 都不应归 CanvasViewport
#[test]
fn test_selected_agent_bodies_scroll_belong_to_terminal_scrollback() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let claude = doc.add_node(
        agent_kind(AgentProfile::Claude),
        CanvasRect::new(100.0, 50.0, 300.0, 220.0),
    );
    let codex = doc.add_node(
        agent_kind(AgentProfile::Codex),
        CanvasRect::new(500.0, 50.0, 300.0, 220.0),
    );
    let opencode = doc.add_node(
        agent_kind(AgentProfile::OpenCode),
        CanvasRect::new(900.0, 50.0, 300.0, 220.0),
    );
    let claude_selection = selected_selection(claude);
    let codex_selection = selected_selection(codex);
    let opencode_selection = selected_selection(opencode);
    let claude_point = screen_point(doc.viewport, 130.0, 80.0);
    let codex_point = screen_point(doc.viewport, 530.0, 80.0);
    let opencode_point = screen_point(doc.viewport, 930.0, 80.0);

    let claude_owner =
        wheel_scroll_owner_for_point(doc.nodes(), &claude_selection, claude_point, doc.viewport);
    let codex_owner =
        wheel_scroll_owner_for_point(doc.nodes(), &codex_selection, codex_point, doc.viewport);
    let opencode_owner = wheel_scroll_owner_for_point(
        doc.nodes(),
        &opencode_selection,
        opencode_point,
        doc.viewport,
    );

    assert_eq!(
        claude_owner,
        WheelScrollOwner::TerminalScrollback(claude),
        "已选 Claude 正文区滚轮应归 terminal scrollback"
    );
    assert_eq!(
        codex_owner,
        WheelScrollOwner::TerminalScrollback(codex),
        "已选 Codex 正文区滚轮应归 terminal scrollback"
    );
    assert_eq!(
        opencode_owner,
        WheelScrollOwner::TerminalScrollback(opencode),
        "已选 OpenCode 正文区滚轮应归 terminal-owned wheel，供 TUI/scrollback 再分流"
    );
    assert_ne!(
        claude_owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Claude 正文区滚轮不应归 CanvasViewport"
    );
    assert_ne!(
        codex_owner,
        WheelScrollOwner::CanvasViewport,
        "已选 Codex 正文区滚轮不应归 CanvasViewport"
    );
    assert_ne!(
        opencode_owner,
        WheelScrollOwner::CanvasViewport,
        "已选 OpenCode 正文区滚轮不应归 CanvasViewport，否则 TUI 无法收到 wheel"
    );
}

/// 未选 Terminal/Agent 正文区命中时，滚轮仍应归画布 viewport。
///
/// 数据构造（含关键数值推导）：
///   terminal bounds = origin(100,50) size(300x220)
///   OpenCode bounds = origin(500,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文 y=80 → screen_y = 170 > title_bottom_screen_y 162
///   selection 为空，因此两个 terminal-backed 卡片都未选中
///
/// 执行过程：
///   1. add Terminal 与 OpenCode Agent
///   2. 不选择任何节点
///   3. 对两个正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：Terminal/OpenCode 都归 CanvasViewport
///   - 负断言：未选卡片不应归 TerminalScrollback，否则 hover 就会抢画布滚轮
#[test]
fn test_unselected_terminal_and_agent_body_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let term = doc.add_node(term_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let opencode = doc.add_node(
        agent_kind(AgentProfile::OpenCode),
        CanvasRect::new(500.0, 50.0, 300.0, 220.0),
    );
    let selection = Selection::new();
    let term_point = screen_point(doc.viewport, 130.0, 80.0);
    let opencode_point = screen_point(doc.viewport, 530.0, 80.0);

    let term_owner =
        wheel_scroll_owner_for_point(doc.nodes(), &selection, term_point, doc.viewport);
    let opencode_owner =
        wheel_scroll_owner_for_point(doc.nodes(), &selection, opencode_point, doc.viewport);

    assert_eq!(
        term_owner,
        WheelScrollOwner::CanvasViewport,
        "未选 Terminal 正文区滚轮应继续归 CanvasViewport"
    );
    assert_eq!(
        opencode_owner,
        WheelScrollOwner::CanvasViewport,
        "未选 OpenCode 正文区滚轮应继续归 CanvasViewport"
    );
    assert_ne!(
        term_owner,
        WheelScrollOwner::TerminalScrollback(term),
        "未选 Terminal 不应抢 terminal scrollback"
    );
    assert_ne!(
        opencode_owner,
        WheelScrollOwner::TerminalScrollback(opencode),
        "未选 OpenCode 不应抢 terminal wheel，否则路过卡片会影响画布滚动"
    );
}

/// wheel 像素 delta 应转换为至少一行的 terminal scrollback 行数。
///
/// 数据构造（含关键数值推导）：
///   约定 TERM_SCROLL_PX_PER_LINE = 16px
///   +32px → ceil(32 / 16) = 2 行，正数表示回看更旧历史
///   -32px → -ceil(32 / 16) = -2 行，负数表示回到底部
///   +1px  → ceil(1 / 16) = 1 行，避免触控板小 delta 被吞
///   0px   → 0 行，不触发 runtime scroll
///
/// 执行过程：
///   1. 分别转换 +32、-32、+1、0
///   2. 检查符号和最小行数
///
/// 预期结果：
///   - 正断言：正 delta 转为正行数，负 delta 转为负行数
///   - 正断言：非零小 delta 至少为 1 行
///   - 负断言：0 delta 不应产生滚动行数
#[test]
fn test_terminal_scroll_lines_from_pixel_delta_preserves_direction_and_minimum() {
    let up = terminal_scroll_lines_from_pixel_delta(32.0);
    let down = terminal_scroll_lines_from_pixel_delta(-32.0);
    let tiny = terminal_scroll_lines_from_pixel_delta(1.0);
    let zero = terminal_scroll_lines_from_pixel_delta(0.0);

    assert_eq!(up, 2, "+32px 应转换为向历史回看的 2 行");
    assert_eq!(down, -2, "-32px 应转换为回到底部方向的 -2 行");
    assert_eq!(tiny, 1, "非零小 delta 也应至少滚动 1 行，避免触控板无响应");
    assert_eq!(zero, 0, "0px delta 不应产生 terminal scroll 行数");
    assert_ne!(zero, 1, "0px delta 不应被最小行数规则误转换为 1 行");
}
