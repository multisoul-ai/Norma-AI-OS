use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{drain_until, make_app_real},
};
use app::{MobileAccess, MobileServer};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use storage::{
    AppStore, MobilePermission, MobileProjectAccess, MobileTaskBaselineStore,
    MobileTaskDeliveryStore, MobileTurnKey, StoredTaskDelivery, TaskBaseline, TaskDeliveryContent,
};

/// 实际WSS直发、Todo确认及排队三次Codex任务，各保存发送前起点；旧快照拒绝不得覆盖记录。
///
/// 数据构造：独立Git项目draft.txt，三轮前依次为first/second/queued draft；固定无工具提示。
/// 执行过程：空卡直发 → Provider确认/完成 → 读取AppDB → Todo接受续发 → 队列发送 → 重放及旧快照拒绝。
/// 预期结果：前三个真实turn绑定原操作/原文且不覆盖交付；第四轮经明确确认可删除运行卡片并回收PTY。
#[test]
#[ignore = "requires real Codex and fresh Hook; isolated repository and no-tool prompts"]
fn e2e_mobile_real_task_baselines_bind_exact_sends() {
    let root = tempfile::tempdir().expect("隔离Git项目");
    let init = std::process::Command::new("/usr/bin/git")
        .args(["init", "--quiet"])
        .current_dir(root.path())
        .status()
        .expect("init");
    assert!(init.success(), "隔离Git初始化必须成功");
    std::fs::write(root.path().join("draft.txt"), "first draft\n").expect("发送前正文");
    let route_dir = tempfile::tempdir().expect("隔离endpoint");
    let mut test = TestAppContext::single();
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(route_dir.path().join("norma.sock"), cx)
            .expect("正式endpoint")
    });
    let (app, cx) = make_app_real(&mut test);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(root.path(), window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        //640×400为桌面卡片默认尺寸；创建时不启动Provider。
        app.runtime
            .create_mobile_agent_card(
                AgentProfile::Codex,
                None,
                None,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("受控空卡")
    });
    let (database, identity, snapshots, commands) = app.read_with(cx, |app, cx| {
        (
            app.app_database_path_for_e2e(),
            app.norma_workspace_identity_for_e2e().expect("真实项目"),
            app.mobile_snapshot_source_for_e2e(cx),
            app.mobile_command_source_for_e2e(cx),
        )
    });
    let store = AppStore::new(database.clone());
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: identity.0.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("测试授权");
    let tls = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = tls.certificate_der().to_vec();
    let machine = tls.machine_id();
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, tls),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("正式WSS");
    let blank = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let command = |operation: &str, expected: &norma::mobile::CardSnapshot| {
        json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
            "operation_id":operation,"workspace_id":identity.0,"route_id":identity.1,"grant_revision":1,
            "command":{"type":"send_message","expected":expected,"text":"Do not use tools or read or modify files. Reply exactly: BASELINE_OK"}
        }})
    };
    let query = |operation: &str| json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":identity.0,"operation_id":operation});
    fetch_action(cx, &server, &cert, &machine, command("first", &blank));
    let first = wait_receipt(cx, &server, &cert, &machine, query("first"));
    let first_key = turn_key(&first);
    let baselines = MobileTaskBaselineStore::new(database.clone());
    let original = baselines
        .get(&identity.0, &first_key)
        .expect("读取落盘")
        .expect("真实回合起点");
    assert_eq!(original.operation_id, "first", "起点绑定原始操作");
    assert_eq!(
        draft(&original.baseline),
        b"first draft\n",
        "首轮必须保留发送前正文"
    );
    //120s为实际模型完成预算，与现有Provider测试一致；不把已收到当作完成。
    let current = drain_until(
        &app,
        cx,
        |r| {
            let card = r.mobile_card_snapshots().remove(0);
            (card.state == "done").then_some(card)
        },
        Duration::from_secs(120),
    )
    .expect("第一轮实际完成");
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.mobile_latest_turn_key(node)),
        Some(first_key.clone()),
        "完成后仍保留准确回合用于冻结交付，不能靠活动turn推断"
    );
    let deliveries = MobileTaskDeliveryStore::new(database.clone());
    let first_delivery = wait_delivery(&app, cx, &deliveries, &identity.0, &first_key);
    assert!(
        matches!(&first_delivery.content, TaskDeliveryContent::Available { review } if review.files.is_empty()),
        "无工具任务的起点与结束相同，不能把用户草稿当成本轮产物"
    );
    let delivery_scope = json!({"workspace_id":identity.0,"route_id":identity.1});
    let list_query = json!({"type":"task_deliveries","device_id":"phone-a","token":"a".repeat(64),"request":delivery_scope,"node_id":node});
    let history = fetch_action(cx, &server, &cert, &machine, list_query.clone());
    assert_eq!(
        history["result"]["deliveries"]["items"][0]["fingerprint"], first_delivery.fingerprint,
        "手机经正式WSS可看到准确交付"
    );
    let read_query = json!({"type":"project_review","device_id":"phone-a","token":"a".repeat(64),"request":{"workspace_id":identity.0,"route_id":identity.1,"delivery_key":first_key}});
    let read = fetch_action(cx, &server, &cert, &machine, read_query.clone());
    assert_eq!(
        read["result"]["review"]["delivery"]["fingerprint"], first_delivery.fingerprint,
        "正文必须带原始交付指纹"
    );
    let mut denied = list_query;
    denied["token"] = json!("b".repeat(64));
    assert!(
        fetch_action(cx, &server, &cert, &machine, denied)
            .get("error")
            .is_some(),
        "无授权不能读取交付摘要"
    );
    std::fs::write(root.path().join("draft.txt"), "second draft\n").expect("第二轮前修改");
    storage::ProjectPlanStore::new(database)
        .publish(
            "baseline-plan",
            &identity.0,
            &identity.1,
            "核验Todo起点",
            &agent::ProjectPlanDraft {
                summary: "隔离任务".into(),
                todos: vec![agent::PlanTodo {
                    target: current.clone(),
                    title: "第二轮".into(),
                    instruction:
                        "Do not use tools or read or modify files. Reply exactly: TODO_BASELINE_OK"
                            .into(),
                }],
            },
        )
        .expect("准确Todo fixture");
    let mut todo = command("second", &current);
    todo["request"]["command"] = json!({"type":"decide_plan_todo", "plan_id":"baseline-plan", "todo_index":0,
        "choice":{"type":"accept", "target":current, "queue":false}});
    let accepted_todo = fetch_action(cx, &server, &cert, &machine, todo);
    assert!(
        accepted_todo.get("error").is_none(),
        "Todo必须沿正式确认链路接收：{accepted_todo}"
    );
    let second = wait_receipt(cx, &server, &cert, &machine, query("second"));
    let second_key = turn_key(&second);
    let saved_second = baselines
        .get(&identity.0, &second_key)
        .expect("第二轮")
        .expect("第二轮起点");
    assert_eq!(
        draft(&saved_second.baseline),
        b"second draft\n",
        "续发必须捕获新起点"
    );
    assert_eq!(
        second_key.session_id, first_key.session_id,
        "续发保持准确会话"
    );
    assert!(
        second_key.turn_id != first_key.turn_id,
        "两次真实任务必须有不同回合"
    );
    let replay = fetch_action(cx, &server, &cert, &machine, command("first", &blank));
    assert_eq!(
        replay["result"]["receipt"]["result"], first["result"]["receipt"]["result"],
        "重放只返回原回执"
    );
    drain_until(
        &app,
        cx,
        |r| (r.mobile_card_snapshots()[0].state == "done").then_some(()),
        Duration::from_secs(120),
    )
    .expect("第二轮完成");
    wait_delivery(&app, cx, &deliveries, &identity.0, &second_key);
    let current = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    std::fs::write(root.path().join("draft.txt"), "queued draft\n").expect("排队前正文");
    let mut queue = command("queued", &current);
    queue["request"]["command"]["type"] = json!("queue_message");
    let queued = fetch_action(cx, &server, &cert, &machine, queue);
    assert!(queued.get("error").is_none(), "入队必须成功：{queued}");
    let queued_query = json!({"type":"queued_message","device_id":"phone-a","token":"a".repeat(64),"workspace_id":identity.0,"operation_id":"queued"});
    //40s实际发送 +0.5s队列轮询 +4.5s网络余量 =45s，不自动重试原命令。
    let deadline = Instant::now() + Duration::from_secs(45);
    let queued = loop {
        let value = fetch_action(cx, &server, &cert, &machine, queued_query.clone());
        let state = value["result"]["queued_message"]["state"].as_str();
        if !matches!(state, Some("queued" | "dispatching")) {
            break value;
        }
        assert!(Instant::now() < deadline, "队列实际发送必须有界");
    };
    assert_eq!(
        queued["result"]["queued_message"]["state"], "sent",
        "必须取得真实队列发送结果：{queued}"
    );
    let mut key = queued["result"]["queued_message"]["result"].clone();
    key["profile"] = json!("Codex");
    let queued_key: MobileTurnKey = serde_json::from_value(key).expect("准确排队回合");
    let queued_baseline = baselines
        .get(&identity.0, &queued_key)
        .expect("查询")
        .expect("实际发送的起点");
    assert_eq!(
        queued_baseline.operation_id, "queued",
        "派发须保留原排队操作，不冒充新指令"
    );
    assert_eq!(
        draft(&queued_baseline.baseline),
        b"queued draft\n",
        "实际发送捕获本轮起点"
    );
    assert!(
        queued_key.turn_id != second_key.turn_id,
        "队列必须产生新的实际回合"
    );
    drain_until(
        &app,
        cx,
        |r| (r.mobile_card_snapshots()[0].state == "done").then_some(()),
        Duration::from_secs(120),
    )
    .expect("排队任务完成");
    wait_delivery(&app, cx, &deliveries, &identity.0, &queued_key);
    let old_read = fetch_action(cx, &server, &cert, &machine, read_query);
    assert_eq!(
        old_read["result"]["review"], read["result"]["review"],
        "后续任务后手机仍读取原始成果，不能重新采集工作副本"
    );
    assert_eq!(
        deliveries.get(&identity.0, &first_key).expect("交付查询"),
        Some(first_delivery),
        "后续任务不得改变首轮已冻结成果"
    );
    fetch_action(cx, &server, &cert, &machine, command("stale", &blank));
    let stale = wait_receipt(cx, &server, &cert, &machine, query("stale"));
    assert_eq!(
        stale["result"]["receipt"]["error"], "card_changed",
        "捕获之后仍必须最终验证卡片"
    );
    assert!(
        stale["result"]["receipt"]["result"].is_null(),
        "拒绝不得伪造发送身份"
    );
    assert_eq!(
        baselines
            .get(&identity.0, &first_key)
            .expect("最终读")
            .expect("原记录"),
        original,
        "第二轮及拒绝不得改写首轮起点"
    );
    assert_eq!(
        baselines
            .get(&identity.0, &second_key)
            .expect("最终读")
            .expect("第二条"),
        saved_second,
        "拒绝不得覆盖第二轮记录"
    );
    let ready = app.read_with(cx, |app, _| app.runtime.mobile_card_snapshots().remove(0));
    let mut running = command("delete-task", &ready);
    running["request"]["command"]["text"] = json!("Do not use tools or modify files. Write at least 10000 words explaining database transactions.");
    fetch_action(cx, &server, &cert, &machine, running);
    wait_receipt(cx, &server, &cert, &machine, query("delete-task"));
    //真实回合已确认后2s检查内存状态；长回答只用于保持可确认删除的活动回合。
    let busy = drain_until(
        &app,
        cx,
        |r| {
            r.mobile_card_snapshots()
                .into_iter()
                .find(|c| c.state == "thinking")
        },
        Duration::from_secs(2),
    )
    .expect("真实任务应正在执行");
    let mut delete = command("delete-running", &busy);
    delete["request"]["command"] =
        json!({"type":"delete_card","expected":busy,"confirm_busy":true});
    fetch_action(cx, &server, &cert, &machine, delete);
    let removed = wait_receipt(cx, &server, &cert, &machine, query("delete-running"));
    assert_eq!(
        removed["result"]["receipt"]["state"], "succeeded",
        "跟踪交付不能禁用用户已确认的运行卡片删除：{removed}"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime.document.node(node).is_none(),
            "删除应真实移除卡片"
        );
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "删除必须回收Provider"
        );
    });
}

fn draft(baseline: &TaskBaseline) -> &[u8] {
    let TaskBaseline::Available { review } = baseline else {
        panic!("小型Git项目应可捕获：{baseline:?}");
    };
    &review
        .files
        .iter()
        .find(|f| f.path == "draft.txt")
        .expect("准确草稿")
        .after
        .as_ref()
        .expect("起点正文")
        .bytes
}
fn turn_key(value: &Value) -> MobileTurnKey {
    assert_eq!(
        value["result"]["receipt"]["state"], "succeeded",
        "必须来自真实发送回执：{value}"
    );
    let mut key = value["result"]["receipt"]["result"].clone();
    key["profile"] = json!("Codex");
    serde_json::from_value(key).expect("准确Provider身份")
}
fn wait_receipt(
    cx: &mut gpui::VisualTestContext,
    server: &MobileServer,
    cert: &[u8],
    machine: &str,
    query: Value,
) -> Value {
    //40s发送预算 +5s查询调度余量，不无限等待或重发。
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let value = fetch_action(cx, server, cert, machine, query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            return value;
        }
        assert!(Instant::now() < deadline, "真实发送回执必须有界完成");
    }
}

fn wait_delivery(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    store: &MobileTaskDeliveryStore,
    workspace: &str,
    key: &MobileTurnKey,
) -> StoredTaskDelivery {
    //10s捕获 +2s审计等待 +3s测试调度 =15s；期间驱动正式前后台任务。
    drain_until(
        app,
        cx,
        |_| store.get(workspace, key).expect("读取不可变交付"),
        Duration::from_secs(15),
    )
    .expect("回合结束必须自动保存交付，不等待手机打开")
}
