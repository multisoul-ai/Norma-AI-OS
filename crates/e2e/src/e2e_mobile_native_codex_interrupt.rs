use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

/// 原生Codex停止必须有准确新增rollout事件，不能相信独立host重建的活动状态。
///
/// 数据构造：原生80×24PTY/640×400卡，已信任cwd，无工具长回答；另一个host只read/list。
/// 执行过程：原卡生成 → 读取准确thread/turn状态 → 原PTY Ctrl-C → 再读同turn → 关闭两进程。
/// 预期结果：停止前没有本回合turn_aborted，Ctrl-C后准确追加；记录read/list差异，不用其默认状态确认停止。
#[test]
#[ignore = "requires real native Codex and fresh Hook; read-only status evidence probe"]
fn e2e_original_codex_readonly_stop_evidence() {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(
                AgentProfile::Codex,
                CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("原生卡")
    });
    crate::e2e_mobile_messages::prime(&app, cx, node, AgentProfile::Codex, true);
    let key = app.read_with(cx, |app, _| {
        app.runtime.mobile_latest_turn_key(node).expect("真实回合")
    });
    let dir = tempfile::tempdir().expect("只读host目录");
    let socket = dir.path().join("read.sock");
    let command = terminal::PtyCommand::new("codex")
        .args([
            "app-server",
            "--listen",
            &format!("unix://{}", socket.display()),
        ])
        .cwd(workspace);
    let mut reader =
        terminal::TerminalSession::spawn_command(1, command, 80, 24).expect("只读host");
    // 30s是既有本地Provider冷启动上限；原卡继续被drain，不阻塞其Hook。
    drain_until(
        &app,
        cx,
        |_| socket.exists().then_some(()),
        Duration::from_secs(30),
    )
    .expect("只读socket");
    let client = norma::codex::CodexConnection::connect(&socket).expect("只读连接");
    let mut rpc = |method: &str, params: Value| {
        let reply = client.request(method, params).expect("只读请求");
        drain_until(&app, cx, |_| reply.try_recv().ok(), Duration::from_secs(30))
            .expect("只读响应")
            .expect("只读结果")
    };
    let metadata = rpc(
        "thread/read",
        json!({"threadId":key.session_id,"includeTurns":false}),
    );
    assert_eq!(metadata["thread"]["id"], key.session_id, "必须读取原thread");
    let path = metadata["thread"]["path"]
        .as_str()
        .expect("本次隔离会话rollout")
        .to_owned();
    println!(
        "NATIVE_CODEX_SESSION {}",
        json!({"session":key.session_id,"path":path})
    );
    let before = rpc(
        "thread/turns/list",
        json!({"threadId":key.session_id,"limit":1,"itemsView":"full"}),
    );
    let timeline_before = rpc(
        "thread/timeline/list",
        json!({"threadId":key.session_id,"limit":100}),
    );
    println!(
        "NATIVE_CODEX_TIMELINE_BEFORE {}",
        boundaries(&timeline_before)
    );
    println!(
        "NATIVE_CODEX_ACTIVE {}",
        json!({"id":before["data"][0]["id"],"status":before["data"][0]["status"]})
    );
    let initial = std::fs::read_to_string(&path).expect("仅本次新建会话");
    println!("NATIVE_CODEX_INITIAL_KINDS {}", json!(initial.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|line| json!({"type":line["type"],"payload_type":line["payload"]["type"],"turn_id":line["payload"]["turn_id"]})).collect::<Vec<_>>()));
    assert!(
        !initial
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|line| line["type"] == "event_msg"
                && line["payload"]["type"] == "turn_aborted"
                && line["payload"]["turn_id"] == key.turn_id),
        "发送停止前不能已有本回合中断"
    );
    let offset = std::fs::metadata(&path).expect("原rollout").len();
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(
                node,
                runtime::NodeInput::Key(terminal::KeyInput {
                    key: terminal::Key::Char('c'),
                    ctrl: true,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("只对原PTY停止")
    });
    let stopped = drain_until(
        &app,
        cx,
        |_| {
            let tail = read_tail(&path, offset);
            tail.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find(|line| {
                    line["type"] == "event_msg"
                        && line["payload"]["type"] == "turn_aborted"
                        && line["payload"]["turn_id"] == key.turn_id
                })
        },
        Duration::from_secs(30),
    );
    let reply = client
        .request(
            "thread/turns/list",
            json!({"threadId":key.session_id,"limit":1,"itemsView":"full"}),
        )
        .expect("只读终态");
    let after = drain_until(&app, cx, |_| reply.try_recv().ok(), Duration::from_secs(30))
        .expect("终态响应")
        .expect("终态读取");
    let reply = client
        .request(
            "thread/timeline/list",
            json!({"threadId":key.session_id,"limit":100}),
        )
        .expect("时间线");
    let timeline_after = drain_until(&app, cx, |_| reply.try_recv().ok(), Duration::from_secs(30))
        .expect("时间线返回")
        .expect("时间线读取");
    println!(
        "NATIVE_CODEX_TIMELINE_AFTER {}",
        boundaries(&timeline_after)
    );
    println!(
        "NATIVE_CODEX_AFTER {}",
        json!({"id":after["data"][0]["id"],"status":after["data"][0]["status"]})
    );
    app.read_with(cx, |app, _| {
        if let Some(runtime::NodeView::Term { snapshot, .. }) = app.runtime.node_view(node) {
            println!(
                "NATIVE_CODEX_TERMINAL {}",
                snapshot
                    .rows
                    .iter()
                    .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    });
    let tail = read_tail(&path, offset);
    for line in tail
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        if line["type"] == "event_msg" {
            println!(
                "NATIVE_CODEX_EVENT {}",
                json!({"type":line["payload"]["type"], "turn_id":line["payload"]["turn_id"], "reason":line["payload"]["reason"]})
            );
        }
    }
    reader.kill();
    app.update(cx, |app, _| app.runtime.kill_all());
    assert_eq!(
        before["data"][0]["id"], key.turn_id,
        "活动状态必须对应原turn"
    );
    assert!(stopped.is_some(), "必须有本次新增的准确turn_aborted证据");
    assert_eq!(
        after["data"][0]["id"], key.turn_id,
        "停止证据不能指向另一回合"
    );
    assert_eq!(
        after["data"][0]["status"], "interrupted",
        "必须区别自然完成和实际中断"
    );
    assert_ne!(
        after["data"][0]["status"], "completed",
        "Ctrl-C不能伪报自然完成"
    );
}

fn read_tail(path: &str, offset: u64) -> String {
    use std::io::{Read, Seek};
    let mut file = std::fs::File::open(path).expect("本次rollout");
    file.seek(std::io::SeekFrom::Start(offset))
        .expect("本次新增位置");
    let mut tail = String::new();
    // 1MiB大于此次无工具短时生成，仅探针使用；产品读取必须处理行边界和部分UTF8。
    file.take(1024 * 1024)
        .read_to_string(&mut tail)
        .expect("最多1MiB新增数据");
    tail
}

fn boundaries(value: &Value) -> Value {
    json!(value["data"]
        .as_array()
        .expect("时间线数组")
        .iter()
        .filter(|entry| matches!(
            entry["type"].as_str(),
            Some("turnStarted" | "turnCompleted")
        ))
        .cloned()
        .collect::<Vec<_>>())
}
