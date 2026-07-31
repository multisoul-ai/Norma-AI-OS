//! E2E-02：Agent 卡片的多行文本输入必须使用终端 CRLF 语义。

use std::time::Duration;

use gpui::TestAppContext;
use runtime::NodeInput;

use crate::harness::{
    create_codex_agent, drain_until, echo_factory, enter_canvas, make_app, read_grid,
};

/// E2E-02：Codex Agent 卡片收到含 LF 的文本粘贴后，第二行必须在第 0 列完整显示。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent cards = 1（经真实 NewCodexAgent action 创建）
///   input = "first\\nsecond" = 5 + 1 + 6 = 12 bytes
///   logical lines = 2；canonical terminal separator = CRLF = 2 control bytes
///   visible rows = 2：row0 前 6 cells = "first "，row1 前 6 cells = "second"
///   drain timeout = 500ms；L1 echo 同步回吐，500ms / 16ms runtime tick ≈ 31 帧，
///                   大于创建卡片、写 PTY 与一次 frame drain 所需的帧数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配带 echo PTY 的 headless GPUI App，并进入临时 Canvas workspace。
///   2. 通过公开 `NewCodexAgent` action 创建 Agent 卡片，使其复用生产 TermLive 输入路径。
///   3. 向该 Agent 节点发送 `NodeInput::Text("first\\nsecond")`。
///   4. echo PTY 回灌实际写入的 bytes；App frame loop drain 后读取 Agent 的 GridSnapshot。
///   5. 检查两行前六个 cells，验证换行不会只下移光标而让第二行从第 5 列开始。
///
/// 预期结果：
///   - 断言 A：第一行精确显示 "first "，证明多行输入首行未丢失。
///   - 断言 B：第二行精确显示 "second"，证明 Agent 卡片每行从左边界开始。
///   - 断言 C：第二行首格不是空格，且不应回归为 "     s" 的裸 LF 横向错位。
#[test]
fn e2e_02_codex_agent_multiline_input_restarts_each_line_at_left_edge() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent 多行输入 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, _cx| {
        app.runtime
            .send_node_input(node_id, NodeInput::Text("first\nsecond".to_string()))
            .expect("Agent 卡片应接受含 LF 的多行 NodeInput::Text")
    });

    let rows = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let first_row = snapshot.rows[0]
                    .iter()
                    .take(6)
                    .map(|cell| cell.c)
                    .collect::<String>();
                let second_row = snapshot.rows[1]
                    .iter()
                    .take(6)
                    .map(|cell| cell.c)
                    .collect::<String>();
                (first_row == "first " && second_row == "second").then_some((first_row, second_row))
            }
            _ => None,
        },
        Duration::from_millis(500),
    )
    .expect("500ms 内 Agent 多行输入应经 PTY 与 frame drain 显示为两行");

    assert_eq!(
        rows.0, "first ",
        "Agent 卡片首行应保留 first，实际前六格为 {:?}",
        rows.0
    );
    assert_eq!(
        rows.1, "second",
        "Agent 卡片第二行必须从第 0 列显示 second，实际前六格为 {:?}",
        rows.1
    );

    let snapshot =
        read_grid(&app, cx, node_id).expect("Agent 节点应保留可读取的 terminal GridSnapshot");
    assert_ne!(
        snapshot.rows[1][0].c, ' ',
        "第二行第 0 列必须是 s，不能因裸 LF 继承上一行列位置而留空"
    );
    let second_row: String = snapshot.rows[1].iter().take(6).map(|cell| cell.c).collect();
    assert_ne!(
        second_row, "     s",
        "第二行不得回归为裸 LF 的五列缩进，实际前六格为 {second_row:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
