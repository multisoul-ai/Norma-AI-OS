use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::{path::Path, time::Duration};

/// 真实Codex的小视口空卡在启动头部滚出屏幕或省略目录后仍可收到手机首条消息。
///
/// 数据构造：1个独立数据库/PTY，80×8视口使model或directory头部不可见；30s启动、60s=模型45s+Hook15s。
/// 执行过程：真实原CLI启动 → 手机快照ready → 通过原卡发送固定回复请求 → 准确回执/终态 → 回收。
/// 预期结果：无会话时就可输入，首条消息产生真实会话并完成；不能另开PTY或修改项目。
#[test]
#[ignore = "requires real Codex and an explicit NORMA_PROGRESS_EVIDENCE directory"]
fn native_codex_small_card_receives_first_mobile_message() {
    let evidence = std::env::var("NORMA_PROGRESS_EVIDENCE").expect("显式证据目录");
    let output = Path::new(&evidence);
    assert!(
        output.is_absolute() && output.is_dir(),
        "仅写已存在的证据目录"
    );
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, root);
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_mobile_agent_card(
                AgentProfile::Codex,
                None,
                CanvasRect::new(0., 0., 640., 180.),
                80,
                8,
            )
            .expect("隔离小卡")
    });
    let ready = drain_until(
        &app,
        cx,
        |r| {
            let card = r.mobile_card_snapshots().remove(0);
            (card.state == "ready").then_some(card)
        },
        Duration::from_secs(30),
    );
    let screen = app.read_with(cx, |app, _| match app.runtime.node_view(node) {
        Some(runtime::NodeView::Term { snapshot, .. }) => snapshot
            .rows
            .iter()
            .map(|row| row.iter().map(|c| c.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "No terminal".into(),
    });
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(output.join("codex-small-card-screen.txt"))
        .expect("保存真实可见区");
    file.write_all(screen.as_bytes()).expect("写屏幕证据");
    if ready.is_none() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    let card = ready.expect("小视口不应卡在unknown");
    assert!(
        !screen.contains("model:") || !screen.contains("directory:"),
        "真实小屏必须缺少至少一项启动字段，才覆盖滚屏或紧凑布局的失败条件"
    );
    assert!(card.session_id.is_none(), "不能提前伪造Codex会话");
    let sent = app.update(cx, |app, _| app.runtime.send_mobile_message(&card,
        "Reply with exactly NORMA_READY_DONE. Do not use any tools, run commands, read or modify files.".into()));
    if sent.is_err() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    let receipt = sent.expect("手机首条输入");
    let final_card = drain_until(
        &app,
        cx,
        |r| {
            let card = r.mobile_card_snapshots().remove(0);
            (card.state == "done").then_some(card)
        },
        Duration::from_secs(60),
    );
    let terminals = app.read_with(cx, |app, _| app.runtime.live_len());
    app.update(cx, |app, _| app.runtime.kill_all());
    let final_card = final_card.expect("原会话真实结束");
    assert_eq!(final_card.run_id, card.run_id, "首条输入不能更换运行实例");
    assert!(final_card.session_id.is_some(), "真实提交后才生成会话");
    assert_eq!(
        receipt.try_recv().expect("回执").expect("成功")["session_id"].as_str(),
        final_card.session_id.as_deref(),
        "输入回执属于同一会话"
    );
    assert_eq!(terminals, 1, "没有新执行器");
}
