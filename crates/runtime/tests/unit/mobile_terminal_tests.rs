use super::*;
use crate::NodeInput;
use norma::mobile::{TerminalInput, TerminalRequest};
use std::sync::Mutex;

/// 手机重排只连接终端自动折行，显式换行与空格不丢失。
///
/// 数据构造：8列原PTY；"hello   world"共13列=8列首行+5列续行，随后CRLF和hard行。
/// 执行过程：向原PTY注入文字 → 读取手机页 → 比较折行标记及桌面宽度 → 仅修改投影中的折行标记。
/// 预期结果：保留软折行、3个空格和硬换行；电脑仍为8列、一个PTY；标记变化不能误判为空闲心跳。
#[test]
fn mobile_terminal_retains_soft_wraps_for_phone_reflow() {
    let output = Arc::new(Mutex::new(None));
    let sender = output.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        *sender.lock().expect("输出") = Some(tx.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            Arc::new(Mutex::new(Vec::new())),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            8,
            4,
        )
        .expect("原卡");
    let card = runtime.mobile_card_snapshots().remove(0);
    output
        .lock()
        .expect("输出")
        .as_ref()
        .expect("原PTY")
        .send(terminal::PtyEvent::Output(
            b"hello   world\r\nhard".to_vec(),
        ))
        .expect("输出文字");
    runtime.drain_all();
    let scope = TerminalRequest {
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        node_id: node,
        run_id: card.run_id.expect("原run"),
        session_id: None,
        before: None,
        revision: None,
    };
    let page = runtime.mobile_terminal_page(&scope).expect("手机页");
    let wire = serde_json::to_value(&page).expect("接口序列化");
    assert_eq!(wire["soft_wrapped"][0], true, "首行必须携带自动折行信息");
    assert_eq!(wire["soft_wrapped"][1], false, "显式CRLF不得当成自动折行");
    assert_eq!(wire["soft_wrapped"][2], false, "短行不得合并到上一段");
    assert_eq!(
        page.rows[0], "hello   ",
        "软折行末尾空格不能被吞掉，否则hello和world会连在一起"
    );
    assert_eq!(page.rows[1], "world", "续行文字必须完整");
    assert_eq!(page.rows[2], "hard", "显式换行后的文字必须完整");
    assert_eq!(page.cols, 8, "手机重排不能改变电脑PTY列数");
    assert_eq!(runtime.live_len(), 1, "手机阅读不得创建第二个PTY");
    let mut projection = norma::mobile::TerminalProjection::new("stream-a".into(), scope);
    projection.update(page.clone()).expect("首屏事件");
    let mut next = page;
    next.soft_wrapped[0] = false;
    let event = projection.update(next).expect("标记更新");
    let delta = event.page.expect("标记变化必须发送事件，不能当作心跳");
    assert!(delta.rows.is_empty(), "标记变化不必重复发送未变化的文字");
    assert_eq!(
        delta.soft_wrapped,
        vec![false; 4],
        "增量仍携带完整页面的折行标记"
    );
}

/// 手机终端复用原PTY；无回显的电脑输入也使旧屏幕按键失效。
///
/// 数据构造：80×24录制PTY，仅1张Codex卡；200行历史=120行当前页+80行更早页。
/// 执行过程：读取→文字/快捷键→重放旧屏幕→电脑输入→过期输入→滚动电脑→手机读最新/历史→换run。
/// 预期结果：准确字节只写一次；旧屏幕/旧run/控制字符不写入；读历史不移动电脑viewport或新建PTY。
#[test]
fn mobile_terminal_same_pty_input_and_scrollback() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::new(Mutex::new(None));
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let recorded = writes.clone();
    let sender = output.clone();
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        *sender.lock().expect("输出") = Some(tx.clone());
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            recorded.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原卡");
    let card = runtime.mobile_card_snapshots().remove(0);
    let mut scope = TerminalRequest {
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        node_id: node,
        run_id: card.run_id.clone().expect("原run"),
        session_id: None,
        before: None,
        revision: None,
    };
    let first = runtime.mobile_terminal_page(&scope).expect("首屏");
    output
        .lock()
        .expect("输出")
        .as_ref()
        .expect("原PTY")
        .send(terminal::PtyEvent::Output(b"\x1b[?25l\x1b[?25h".to_vec()))
        .expect("仅光标重绘");
    runtime.drain_all();
    let repaint = runtime.mobile_terminal_page(&scope).expect("同一画面");
    assert_ne!(repaint.revision, first.revision, "原始渲染版本确实推进");
    assert_eq!(
        repaint.screen_signature, first.screen_signature,
        "没有可见变化的重绘不应误拒输入"
    );
    let text = TerminalInput::Text {
        text: "hello 中文".into(),
    };
    runtime
        .send_mobile_terminal_input(&card, first.screen_signature, first.input_sequence, &text)
        .expect("文字写原PTY");
    assert_eq!(
        *writes.lock().expect("写入"),
        vec!["hello 中文".as_bytes().to_vec()],
        "文字不应附带Enter或新开执行器"
    );
    assert_eq!(
        runtime.send_mobile_terminal_input(
            &card,
            first.screen_signature,
            first.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_changed"),
        "同屏旧按键不能重发"
    );
    let current = runtime
        .mobile_terminal_page(&scope)
        .expect("无回显也更新输入序号");
    runtime
        .send_node_input(node, NodeInput::Text(" desktop".into()))
        .expect("电脑输入");
    assert_eq!(
        runtime.send_mobile_terminal_input(
            &card,
            current.screen_signature,
            current.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_changed"),
        "电脑输入必须使旧手机状态失效"
    );
    let current = runtime.mobile_terminal_page(&scope).expect("当前");
    runtime
        .send_mobile_terminal_input(
            &card,
            current.screen_signature,
            current.input_sequence,
            &TerminalInput::Up,
        )
        .expect("方向键");
    assert_eq!(
        writes.lock().expect("写入").last(),
        Some(&b"\x1b[A".to_vec()),
        "方向键复用原编码器"
    );
    assert!(
        runtime
            .send_mobile_terminal_input(
                &card,
                current.screen_signature,
                current.input_sequence,
                &TerminalInput::Text {
                    text: "bad\rcommand".into()
                }
            )
            .is_err(),
        "文字不得隐式执行控制键"
    );
    let history: String = (0..200).map(|i| format!("history-{i:03}\r\n")).collect();
    output
        .lock()
        .expect("输出")
        .as_ref()
        .expect("同PTY")
        .send(terminal::PtyEvent::Output(history.into_bytes()))
        .expect("输出历史");
    runtime.drain_all();
    runtime
        .scroll_node(node, crate::NodeScroll::TermLines(10))
        .expect("电脑回看10行");
    let page = runtime
        .mobile_terminal_page(&scope)
        .expect("手机独立最新页");
    assert!(
        page.rows.iter().any(|s| s.contains("history-199")),
        "手机应读取最新输出"
    );
    assert!(
        !page.rows.iter().any(|s| s.contains("history-000")),
        "最新页只复制有界尾部"
    );
    scope.before = Some(page.start_line);
    scope.revision = Some(page.revision);
    let older = runtime.mobile_terminal_page(&scope).expect("回看历史");
    assert!(
        older.rows.iter().any(|s| s.contains("history-000")),
        "可读取更早历史"
    );
    let crate::NodeView::Term { snapshot, .. } = runtime.node_view(node).expect("电脑终端");
    assert_eq!(snapshot.display_offset, 10, "手机回看不得改变电脑滚动位置");
    scope.run_id = "stale-run".into();
    assert_eq!(
        runtime.mobile_terminal_page(&scope).err(),
        Some("session_changed"),
        "旧run不可读取新终端"
    );
    assert_eq!(runtime.live_len(), 1, "全过程仅一个原PTY");
    assert_eq!(
        writes.lock().expect("写入").len(),
        3,
        "拒绝路径不能产生额外写入"
    );
}
