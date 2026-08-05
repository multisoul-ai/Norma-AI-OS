use super::*;
use crate::live::{LiveNode, NodeInput, NodeResize, NodeScroll, NodeView};
use crossbeam_channel::{unbounded, Sender};
use std::sync::Arc;
use terminal::{PtyBackend, PtyCommand, PtyEvent, TerminalSession, TestPtyBackend};

/// TNV-1：Terminal 未变化时连续 node view 必须复用同一 Arc 和 revision。
///
/// 数据构造（含关键数值的推导过程）：
///   initial cached refresh count = 1
///   consecutive view reads = 3
///   intervening drain/resize/scroll operations = 0
///   expected final refresh count = 1 + 0 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 80×24 的 TermLive 并读取初始 revision r0。
///   2. 连续三次读取 NodeView::Term，不触发任何终端变化。
///   3. 比较三份 Arc 身份、revision 和 cached refresh count。
///
/// 预期结果：
///   - 正断言：三份 snapshot Arc::ptr_eq 均为 true。
///   - 正断言：三份 revision 均等于 r0。
///   - 负断言：refresh count 不得从 1 增长为 3 或 4。
#[test]
fn test_term_node_view_reuses_arc_and_revision_until_terminal_changes() {
    let mut live = TermLive::spawn(41, "/bin/sh", 80, 24).expect("TermLive spawn 应成功");

    let Some(NodeView::Term {
        snapshot: first,
        revision: first_revision,
        ..
    }) = live.view()
    else {
        panic!("首次 view 必须返回 NodeView::Term");
    };
    let Some(NodeView::Term {
        snapshot: second,
        revision: second_revision,
        ..
    }) = live.view()
    else {
        panic!("第二次 view 必须返回 NodeView::Term");
    };
    let Some(NodeView::Term {
        snapshot: third,
        revision: third_revision,
        ..
    }) = live.view()
    else {
        panic!("第三次 view 必须返回 NodeView::Term");
    };

    assert!(
        Arc::ptr_eq(&first, &second),
        "首次与第二次 view 必须共享同一 GridSnapshot Arc"
    );
    assert!(
        Arc::ptr_eq(&second, &third),
        "第二次与第三次 view 必须共享同一 GridSnapshot Arc"
    );
    assert_eq!(
        second_revision, first_revision,
        "Terminal 未变化时第二次 revision 必须保持 r0"
    );
    assert_eq!(
        third_revision, first_revision,
        "Terminal 未变化时第三次 revision 必须保持 r0"
    );
    assert_eq!(
        live.cached_view_refresh_count(),
        1,
        "三次只读 view 后 refresh count 应仍为初始 1"
    );
    assert_ne!(
        live.cached_view_refresh_count(),
        3,
        "只读 view 不得把 refresh count 增加到 3"
    );

    live.kill();
}

/// TNV-2：一次 PTY 输出 drain 只生成一个新 Arc，并让 revision 单调增加一次。
///
/// 数据构造（含关键数值的推导过程）：
///   initial revision = r0
///   echo input = "hi" = 2 bytes
///   dirty drains = 1
///   empty drains after output = 1
///   expected revision sequence = r0 → r1(r1 > r0) → r1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 echo PTY factory 创建 80×24 TermLive 并保存初始 Arc/r0。
///   2. 写入 "hi" 并执行一次 dirty drain，读取新 Arc/r1 和首行。
///   3. 再执行一次无输出 drain，读取第三份 Arc/revision。
///
/// 预期结果：
///   - 正断言：dirty drain 返回 true、r1 > r0、新旧 Arc 不同且首行含 hi。
///   - 正断言：空 drain 返回 false，revision 保持 r1 且继续复用第二份 Arc。
///   - 负断言：无输出 drain 不得制造第三个 snapshot frame。
#[test]
fn test_term_node_view_advances_revision_once_for_drained_output() {
    let factory: PtyFactory = Box::new(
        |_cmd: PtyCommand, _cols: u16, _rows: u16, tx: Sender<PtyEvent>| {
            Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
        },
    );
    let mut live = TermLive::spawn_with_factory(42, PtyCommand::new("/bin/sh"), 80, 24, &factory)
        .expect("echo TermLive 应创建成功");
    let Some(NodeView::Term {
        snapshot: initial,
        revision: initial_revision,
        ..
    }) = live.view()
    else {
        panic!("初始 view 必须是 Terminal");
    };

    live.input(NodeInput::Text("hi".to_string()))
        .expect("echo PTY 写入 hi 应成功");
    let first_dirty = live.drain();
    let Some(NodeView::Term {
        snapshot: after_output,
        revision: output_revision,
        ..
    }) = live.view()
    else {
        panic!("输出后 view 必须是 Terminal");
    };
    let first_row: String = after_output.rows[0].iter().map(|cell| cell.c).collect();

    let second_dirty = live.drain();
    let Some(NodeView::Term {
        snapshot: after_empty_drain,
        revision: empty_drain_revision,
        ..
    }) = live.view()
    else {
        panic!("空 drain 后 view 必须是 Terminal");
    };

    assert!(first_dirty, "echo 输出的首次 drain 必须标记 dirty");
    assert!(
        output_revision > initial_revision,
        "dirty drain 后 revision 必须从 r0 单调增加到 r1"
    );
    assert!(
        !Arc::ptr_eq(&initial, &after_output),
        "PTY 输出改变可渲染帧后必须生成新的 GridSnapshot Arc"
    );
    assert!(
        first_row.starts_with("hi"),
        "新 snapshot 首行必须包含 echo 输出 hi，实际为 {first_row:?}"
    );
    assert!(
        !second_dirty,
        "没有新 PTY 事件时第二次 drain 必须返回 false"
    );
    assert_eq!(
        empty_drain_revision, output_revision,
        "空 drain 不得继续增加 terminal revision"
    );
    assert!(
        Arc::ptr_eq(&after_output, &after_empty_drain),
        "空 drain 后必须继续复用上一份 snapshot Arc，不能制造第三帧"
    );

    live.kill();
}

/// TNV-3：Terminal resize 必须发布一个新 Arc/revision，并更新可渲染尺寸。
///
/// 数据构造（含关键数值的推导过程）：
///   initial grid = 80 cols × 24 rows
///   target grid = 100 cols × 30 rows
///   resize operations = 1
///   expected revision = r0 + 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 80×24 TermLive，保存初始 Arc/r0。
///   2. 执行一次 NodeResize::TermGrid{100,30}。
///   3. 读取 resize 后的 Arc/revision/cols/rows。
///
/// 预期结果：
///   - 正断言：revision 精确增加一次，新 snapshot 尺寸为 100×30。
///   - 正断言：resize 前后 Arc 不同。
///   - 负断言：新 snapshot 不得继续暴露旧尺寸 80×24。
#[test]
fn test_term_resize_advances_revision_and_updates_snapshot_dimensions() {
    let mut live = TermLive::spawn(43, "/bin/sh", 80, 24).expect("TermLive spawn 应成功");
    let Some(NodeView::Term {
        snapshot: before,
        revision: before_revision,
        ..
    }) = live.view()
    else {
        panic!("resize 前 view 必须是 Terminal");
    };

    live.resize(NodeResize::TermGrid {
        cols: 100,
        rows: 30,
    })
    .expect("TermGrid resize 应成功");
    let Some(NodeView::Term {
        snapshot: after,
        revision: after_revision,
        ..
    }) = live.view()
    else {
        panic!("resize 后 view 必须是 Terminal");
    };

    assert_eq!(
        after_revision,
        before_revision + 1,
        "一次 resize 应让 terminal revision 从 r0 精确增加到 r0+1"
    );
    assert!(
        !Arc::ptr_eq(&before, &after),
        "resize 改变可渲染尺寸后必须发布新的 GridSnapshot Arc"
    );
    assert_eq!(
        after.cols, 100,
        "resize 后 snapshot.cols 必须更新为目标值 100"
    );
    assert_eq!(
        after.rows.len(),
        30,
        "resize 后 snapshot 必须包含目标行数 30"
    );
    assert_ne!(
        (after.cols, after.rows.len()),
        (80, 24),
        "resize 后 snapshot 不得继续暴露旧尺寸 80×24"
    );

    live.kill();
}

/// TNV-4：Terminal scrollback 变化必须发布新 Arc/revision；到达边界后不得伪造新帧。
///
/// 数据构造（含关键数值的推导过程）：
///   output lines = 5（A0/B1/C2/D3/E4）
///   viewport rows = 3
///   history_size = output lines(5) - viewport rows(3) = 2
///   first scroll = +2 lines：display_offset 0 → 2
///   boundary scroll = +1 line：display_offset 保持 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过确定性测试通道注入五行输出并 drain，保存输出帧 Arc/r1。
///   2. scroll TermLines(2)，读取回看帧 Arc/r2。
///   3. 在历史顶部继续 scroll TermLines(1)，读取边界帧。
///
/// 预期结果：
///   - 正断言：首次 scroll 返回 true、offset=2、revision 精确增加一次且 Arc 改变。
///   - 正断言：边界 scroll 返回 false，并继续复用 r2/同一 Arc。
///   - 负断言：首次 scroll 后 offset 不得仍为 0；边界操作不得制造第三帧。
#[test]
fn test_term_scrollback_change_advances_revision_and_updates_offset() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::echo(tx.clone());
    let session = TerminalSession::spawn_with_backend(44, 4, 3, Box::new(backend), rx);
    let mut live = TermLive::from_session(session).expect("测试 TermLive 应创建成功");
    tx.send(PtyEvent::Output(b"A0\r\nB1\r\nC2\r\nD3\r\nE4".to_vec()))
        .expect("五行测试输出应成功注入");
    assert!(live.drain(), "注入五行输出后 drain 必须标记 dirty");
    let Some(NodeView::Term {
        snapshot: before_scroll,
        revision: before_revision,
        ..
    }) = live.view()
    else {
        panic!("scroll 前 view 必须是 Terminal");
    };

    let moved = live
        .scroll(NodeScroll::TermLines(2))
        .expect("向上回看两行应成功");
    let Some(NodeView::Term {
        snapshot: after_scroll,
        revision: after_revision,
        ..
    }) = live.view()
    else {
        panic!("scroll 后 view 必须是 Terminal");
    };

    let moved_past_top = live
        .scroll(NodeScroll::TermLines(1))
        .expect("历史顶部边界 scroll 应安全返回");
    let Some(NodeView::Term {
        snapshot: after_boundary_scroll,
        revision: boundary_revision,
        ..
    }) = live.view()
    else {
        panic!("边界 scroll 后 view 必须是 Terminal");
    };

    assert_eq!(
        before_scroll.history_size, 2,
        "5 行输出和 3 行 viewport 应形成 2 行 history"
    );
    assert!(moved, "从底部向上滚动两行必须返回 true");
    assert_eq!(
        after_scroll.display_offset, 2,
        "向上滚动两行后 display_offset 必须从 0 更新为 2"
    );
    assert_ne!(
        after_scroll.display_offset, 0,
        "发生 scrollback 变化后 snapshot 不得继续显示底部 offset=0"
    );
    assert_eq!(
        after_revision,
        before_revision + 1,
        "一次有效 scroll 应让 revision 精确增加一次"
    );
    assert!(
        !Arc::ptr_eq(&before_scroll, &after_scroll),
        "scrollback viewport 改变后必须发布新的 GridSnapshot Arc"
    );
    assert!(!moved_past_top, "已在历史顶部时继续向上滚动必须返回 false");
    assert_eq!(
        boundary_revision, after_revision,
        "无效边界 scroll 不得增加 terminal revision"
    );
    assert!(
        Arc::ptr_eq(&after_scroll, &after_boundary_scroll),
        "无效边界 scroll 必须继续复用上一份 snapshot Arc，不能制造第三帧"
    );

    live.kill();
}

/// TNV-5：退出事件与最后一批输出必须原子发布为一个最终 Arc/revision。
///
/// 数据构造（含关键数值的推导过程）：
///   output event = "FINAL" = 5 bytes
///   exit event code = Some(7)
///   events drained in one batch = 2
///   renderable frame publications for that batch = 1
///   expected revision = r0 + 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建确定性测试 session，保存初始 Running 帧 Arc/r0。
///   2. 依次注入 Output("FINAL") 与 Exited{7}，一次 drain 排空整批事件。
///   3. 读取最终 Arc/revision/status 和首行内容。
///
/// 预期结果：
///   - 正断言：drain dirty，最终帧包含 FINAL，status=Exited{Some(7)}。
///   - 正断言：整批事件只让 revision 增加一次，且发布新 Arc。
///   - 负断言：最终状态不得仍为 Running，最终帧不得丢失最后输出。
#[test]
fn test_term_exit_status_publishes_last_frame_with_one_revision() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::echo(tx.clone());
    let session = TerminalSession::spawn_with_backend(45, 80, 24, Box::new(backend), rx);
    let mut live = TermLive::from_session(session).expect("测试 TermLive 应创建成功");
    let Some(NodeView::Term {
        snapshot: initial,
        revision: initial_revision,
        status: initial_status,
    }) = live.view()
    else {
        panic!("退出前 view 必须是 Terminal");
    };

    tx.send(PtyEvent::Output(b"FINAL".to_vec()))
        .expect("最后一批输出应成功注入");
    tx.send(PtyEvent::Exited { code: Some(7) })
        .expect("退出事件应成功注入");
    let dirty = live.drain();
    let Some(NodeView::Term {
        snapshot: final_snapshot,
        revision: final_revision,
        status: final_status,
    }) = live.view()
    else {
        panic!("退出后 view 必须是 Terminal");
    };
    let first_row: String = final_snapshot.rows[0].iter().map(|cell| cell.c).collect();

    assert_eq!(
        initial_status,
        terminal::TerminalStatus::Running,
        "测试前置条件：初始 terminal 状态必须为 Running"
    );
    assert!(dirty, "包含输出和退出事件的一次 drain 必须标记 dirty");
    assert_eq!(
        final_revision,
        initial_revision + 1,
        "同一 drain 批次的输出与退出状态应只发布一个新 revision"
    );
    assert!(
        !Arc::ptr_eq(&initial, &final_snapshot),
        "最终输出或退出状态变化后必须发布新的 GridSnapshot Arc"
    );
    assert!(
        first_row.starts_with("FINAL"),
        "最终 snapshot 必须保留退出前最后输出 FINAL，实际为 {first_row:?}"
    );
    assert_eq!(
        final_status,
        terminal::TerminalStatus::Exited { code: Some(7) },
        "最终 NodeView status 必须保留明确退出码 7"
    );
    assert_ne!(
        final_status,
        terminal::TerminalStatus::Running,
        "已收到退出事件后最终状态不得仍为 Running"
    );

    live.kill();
}
