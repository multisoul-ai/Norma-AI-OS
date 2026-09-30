/// Explicit foreground substitute: production relay/authorization only, not real PTY evidence.
pub fn serve(
    receiver: async_channel::Receiver<app::LiveTerminalRequest>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(call) = receiver.recv_blocking() {
            let _ = call.reply.send(Ok(page(&call)));
        }
    })
}

pub fn page(call: &app::LiveTerminalRequest) -> norma::mobile::TerminalPage {
    page_for_turn(call, "original-turn")
}

pub fn page_for_turn(call: &app::LiveTerminalRequest, turn: &str) -> norma::mobile::TerminalPage {
    let bound = call.task.as_ref().expect("线程终端必须带后台验证过的回合");
    assert_eq!(bound.task_ref, "test-runtime:7", "不能用数字ID挑最新任务");
    assert_eq!(
        bound.key.turn_id, turn,
        "必须从原命令起点推导回合"
    );
    assert_eq!(bound.agent_definition_id, 2, "必须仍为原Agent定义");
    assert_eq!(call.request.route_id, "route-a", "重试不能替换原派工路由");
    assert_eq!(call.request.run_id, "review-run", "必须读取原运行");
    assert_eq!(
        call.request.session_id.as_deref(),
        Some("review-session"),
        "必须读取原会话"
    );
    let end = call.request.before.unwrap_or(200);
    //200行 = 当前尾部120行 + 更早80行，复用正式分页预算。
    let start = end.saturating_sub(120);
    let mut page = norma::mobile::TerminalPage {
        card: serde_json::from_value(
            serde_json::json!({"node_id":2,"profile":"Codex","title":"Fixture",
        "mobile_control":true,"state":"done","run_id":"review-run","session_id":"review-session",
        "turn_id":null,"task_title":"Fixture"}),
        )
        .expect("测试卡片"),
        revision: 9,
        screen_signature: 17,
        input_sequence: 0,
        start_line: start,
        total_lines: 200,
        cols: 80,
        running: true,
        input_ready: true,
        rows: (start..end)
            .map(|i| format!("fixture-terminal-{i}"))
            .collect(),
        soft_wrapped: vec![false; end - start],
        cursor: if call.request.before.is_none() {
            Some(norma::mobile::TerminalCursor {
                line: 199,
                column: 5,
            })
        } else {
            None
        },
    };
    if std::env::var_os("NORMA_TERMINAL_UI_FIXTURE").is_some() && call.request.before.is_none() {
        //80列中第71列，超过iPhone正常/AX5可见宽度；21字符前缀+49字符输入=70列。
        page.cursor.as_mut().expect("最新页光标").column = 70;
        *page.rows.last_mut().expect("最新行") = format!("fixture-terminal-199 {}", "x".repeat(49));
    }
    if call.request.before == Some(199) {
        //120×512×4 UTF-8 bytes =240KiB before JSON metadata: valid grid, oversized relay result.
        page.cols = 512;
        page.rows = vec!["𝔄".repeat(512); 120];
    }
    page
}
