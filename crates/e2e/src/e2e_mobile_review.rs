use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app},
};
use app::{MobileAccess, MobileServer, MobileSources};
use gpui::TestAppContext;
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 原生Swift或iOS客户端经正式GPUI与WSS完整读取当前项目的大文件预览。
///
/// 数据构造：隔离Git未跟踪review.txt，200000汉字×3字节=600000字节；一张Codex空卡无执行器，持久化终态fixture，冻结后工作树改为“后”。
/// 执行过程：正式窗口注册 → 冻结后删除卡片 → control配对 → 原生项目级列表/分片核验旧正文 → 验收/历史摘要回查 → 旧route拒绝。
/// 预期结果：卡片不存在时仍可发现并验收成果，所有字节完整；验收前无标记，之后列表和详情对应同一记录；始终无Agent进程。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn e2e_mobile_review_native_transport() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生客户端");
    let mut test = TestAppContext::single();
    let socket = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket.path().join("norma.sock"), cx).expect("正式路由")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let root = tempfile::tempdir().expect("隔离项目");
    let init = std::process::Command::new("/usr/bin/git")
        .current_dir(root.path())
        .args(["init", "--quiet"])
        .status()
        .expect("Git fixture");
    assert!(init.success(), "Git初始化成功才可预览");
    std::fs::write(root.path().join("review.txt"), "变".repeat(200_000)).expect("真实大文件");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(root.path(), window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Codex,
                None,
                None,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("空卡")
    });
    let (database, workspace) = app.read_with(cx, |app, _| {
        (
            app.app_database_path_for_e2e(),
            app.norma_workspace_identity_for_e2e().expect("项目身份").0,
        )
    });
    let key = storage::MobileTurnKey {
        node_id: node,
        profile: "Codex".into(),
        run_id: "fixture-run".into(),
        session_id: "fixture-session".into(),
        turn_id: "fixture-turn".into(),
    };
    let writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(socket.path().join("audit.sqlite")).with_turn_store(
            storage::MobileTurnStore::for_workspace(database.clone(), workspace.clone()),
        ),
    )
    .expect("正式终态writer");
    //5s审计提交上限；这是明确标注的已完成fixture，不宣称运行真实Provider。
    writer
        .record_turn(storage::MobileTurnOutcome {
            summary: None,
            key: key.clone(),
            state: storage::MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("回执")
        .expect("落盘");
    let review = workspace::review::capture(
        root.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(10),
    )
    .expect("捕获真实600kB内容");
    storage::MobileTaskDeliveryStore::new(database)
        .record(
            &workspace,
            &key,
            &storage::TaskDeliveryContent::Available { review },
        )
        .expect("冻结大交付");
    std::fs::write(root.path().join("review.txt"), "后".repeat(200_000))
        .expect("交付后真实工作树已变化");
    app.update(cx, |app, _| {
        assert!(
            app.runtime.remove_node(node),
            "删除卡片后项目列表必须继续提供已冻结成果"
        );
    });
    crate::e2e_mobile_conversation::verify_native(&app, cx, root.path(), &probe, "reviews");
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "配对与预览都不能隐式派任务"
    );
}

/// 正式GPUI owner将当前授权目录交给后台捕获，返回真实文件；离开项目后旧窗口不再可读。
///
/// 数据构造：两个隔离项目A/B；A是未提交Git仓库，a.txt正文为actual project bytes；只读手机仅授权A。
/// 执行过程：注册A窗口 → WSS后台捕获 → 只读验收拒绝 → 授予control → 准备态拒绝 → 删除空卡 → 项目级成果读取/验收/摘要回查 → 转入B。
/// 预期结果：删除卡片后无需卡片ID仍可发现原成果，历史列表带准确验收且不启动Agent；越权/重复/旧route拒绝，原始决定保留。
#[test]
fn e2e_mobile_review_reads_actual_project_without_starting_agents() {
    let mut test = TestAppContext::single();
    let socket = tempfile::tempdir().expect("路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(socket.path().join("norma.sock"), cx)
            .expect("正式路由注册表")
    });
    let (app, cx) = make_app(&mut test, echo_factory());
    let a = tempfile::tempdir().expect("A");
    let b = tempfile::tempdir().expect("B");
    let init = std::process::Command::new("/usr/bin/git")
        .current_dir(a.path())
        .args(["init", "--quiet"])
        .status()
        .expect("初始化Git");
    assert!(init.success(), "隔离Git初始化必须成功");
    std::fs::write(a.path().join("a.txt"), "actual project bytes\n").expect("真实文件");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(a.path(), window, cx)
        })
    });
    let store = AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(a.path(), "A")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "Phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            terminals: app.read_with(cx, |app, cx| app.mobile_terminal_source_for_e2e(cx)),
            snapshots: app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx)),
            reviews: app.read_with(cx, |app, cx| app.mobile_review_source_for_e2e(cx)),
            commands: app.read_with(cx, |app, cx| app.mobile_command_source_for_e2e(cx)),
            ..Default::default()
        },
    )
    .expect("正式WSS与GPUI source");
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot", "device_id":"phone-a", "token":"a".repeat(64), "workspace_id":workspace}),
    );
    let action = json!({"type":"project_review", "device_id":"phone-a", "token":"a".repeat(64), "request":{"workspace_id":workspace, "route_id":snapshot["result"]["projects"][0]["route_id"]}});
    let result = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        result["result"]["status"], "project_review",
        "正式App捕获应返回成果预览: {result}"
    );
    let files = result["result"]["review"]["review"]["files"]
        .as_array()
        .expect("完整文件列表");
    assert_eq!(
        files.len(),
        1,
        "只包含项目代码，不包含.norma运行状态: {:?}",
        files.iter().map(|f| &f["path"]).collect::<Vec<_>>()
    );
    assert_eq!(files[0]["path"], "a.txt", "路径相对实际授权根");
    let bytes: Vec<u8> =
        serde_json::from_value(files[0]["after"]["bytes"].clone()).expect("完整正文");
    assert_eq!(
        bytes, b"actual project bytes\n",
        "不能用fixture或另一目录冒充实际正文"
    );
    assert!(
        !result.to_string().contains(&a.path().display().to_string()),
        "网络结果不得暴露根目录路径"
    );
    let node = app.update(cx, |app, _| {
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Codex,
                None,
                None,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("历史成果所属空卡")
    });
    let key = storage::MobileTurnKey {
        node_id: node,
        profile: "Codex".into(),
        run_id: "history-run".into(),
        session_id: "history-session".into(),
        turn_id: "history-turn".into(),
    };
    let writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(socket.path().join("audit.sqlite")).with_turn_store(
            storage::MobileTurnStore::for_workspace(store.path().to_path_buf(), workspace.clone()),
        ),
    )
    .expect("真实存储管线");
    //5s为已有后台审计提交预算，fixture不声称执行过真实模型。
    writer
        .record_turn(storage::MobileTurnOutcome {
            summary: None,
            key: key.clone(),
            state: storage::MobileTurnState::Completed,
        })
        .expect("记录终态")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("等待提交")
        .expect("已落盘");
    let delivery = storage::MobileTaskDeliveryStore::new(store.path().to_path_buf())
        .record(
            &workspace,
            &key,
            &storage::TaskDeliveryContent::Available {
                review: serde_json::from_value(result["result"]["review"]["review"].clone())
                    .expect("真实预览"),
            },
        )
        .expect("历史成果");
    let list = json!({"type":"task_deliveries","device_id":"phone-a","token":"a".repeat(64),"node_id":node,"request":{"workspace_id":workspace,"route_id":snapshot["result"]["projects"][0]["route_id"]}});
    let before = fetch_action(cx, &server, &cert, &machine, list.clone());
    assert_eq!(
        before["result"]["status"], "task_deliveries",
        "验收前必须实际读到成果列表，不能把读取失败当无标记"
    );
    assert_eq!(
        before["result"]["deliveries"]["items"]
            .as_array()
            .expect("摘要列表")
            .len(),
        1,
        "列表应包含当前未验收成果"
    );
    assert!(
        before["result"]["deliveries"]["items"][0]["acceptance"].is_null(),
        "已完成成果不能自动带验收标记"
    );
    let mut accept = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{"operation_id":"accept-history","workspace_id":workspace,"route_id":snapshot["result"]["projects"][0]["route_id"],"grant_revision":snapshot["result"]["grant"]["revision"],"command":{"type":"accept_delivery","key":key,"fingerprint":delivery.fingerprint}}});
    let denied = fetch_action(cx, &server, &cert, &machine, accept.clone());
    assert_eq!(
        denied["error"], "unauthorized",
        "只读手机不得验收历史成果: {denied}"
    );
    let grant = server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("仅测试授权升级");
    accept["request"]["grant_revision"] = json!(grant.revision);
    //80×24为标准终端尺寸；这里只预约启动，不能运行模型。
    let launch = app.update(cx, |app, _| {
        app.runtime
            .mobile_agent_launch(node, std::path::Path::new("unused"), 80, 24)
            .expect("准备态")
    });
    let mut during_launch = accept.clone();
    during_launch["request"]["operation_id"] = json!("accept-preparing");
    let mut denied = fetch_action(cx, &server, &cert, &machine, during_launch);
    let launch_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"accept-preparing"});
    //沿用100×20ms=2s的本地命令回执预算。
    for _ in 0..100 {
        if denied["result"]["receipt"]["state"] != "accepted" {
            break;
        }
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
        denied = fetch_action(cx, &server, &cert, &machine, launch_query.clone());
    }
    assert_eq!(
        denied["result"]["receipt"]["error"], "state_not_ready",
        "卡片准备运行时不能验收: {denied}"
    );
    assert!(
        storage::MobileCommandStore::new(store.path().to_path_buf())
            .delivery_acceptance(&workspace, &key)
            .expect("检查未验收")
            .is_none(),
        "执行状态不稳定时不能留下决定"
    );
    let old_target = json!({"node_id":node,"profile":"Codex","title":"old","mobile_control":true,
        "state":"done","run_id":key.run_id,"session_id":key.session_id});
    let mut feedback = accept.clone();
    feedback["request"]["operation_id"] = json!("feedback-old-run");
    feedback["request"]["command"] = json!({"type":"feedback_delivery","expected":old_target,
        "feedback":{"key":key,"fingerprint":delivery.fingerprint,"text":"Do not send to a new session"}});
    let mut feedback_receipt = fetch_action(cx, &server, &cert, &machine, feedback.clone());
    let feedback_query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"feedback-old-run"});
    //10s后台起点捕获+5s本地调度余量；旧会话验证必须位于捕获之后的实际发送入口。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while feedback_receipt["result"]["receipt"]["state"] == "accepted" {
        assert!(std::time::Instant::now() < deadline, "旧反馈必须有界拒绝");
        feedback_receipt = fetch_action(cx, &server, &cert, &machine, feedback_query.clone());
    }
    assert_eq!(
        feedback_receipt["result"]["receipt"]["state"], "rejected",
        "原反馈不能启动准备中的新会话"
    );
    assert_eq!(
        feedback_receipt["result"]["receipt"]["error"], "card_changed",
        "捕获后仍核对原运行快照"
    );
    assert!(
        feedback_receipt["result"]["receipt"]["result"].is_null(),
        "拒绝不能生成新任务回执"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "反馈不能启动未准备的Provider"
    );
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, feedback)["result"]["receipt"]["state"],
        "rejected",
        "重放只能回查拒绝"
    );
    app.update(cx, |app, _| {
        app.runtime
            .cancel_mobile_agent_launch(node, launch.generation());
        assert!(app.runtime.remove_node(node), "移除空卡后仍保留其历史成果");
    });
    drop(launch);

    let mut project_list = list.clone();
    project_list
        .as_object_mut()
        .expect("成果请求")
        .remove("node_id");
    let project_history = fetch_action(cx, &server, &cert, &machine, project_list.clone());
    assert_eq!(
        project_history["result"]["status"], "task_deliveries",
        "删除卡片后必须能从项目发现成果: {project_history}"
    );
    assert!(
        project_history["result"]["deliveries"]["node_id"].is_null(),
        "项目列表不能冒充单卡范围"
    );
    let items = project_history["result"]["deliveries"]["items"]
        .as_array()
        .expect("项目成果摘要");
    assert_eq!(items.len(), 1, "删除不能移除已保存成果，也不能新增任务");
    assert_eq!(
        items[0]["fingerprint"], delivery.fingerprint,
        "保留原成果内容身份"
    );
    let mut foreign_list = project_list.clone();
    foreign_list["request"]["workspace_id"] = json!("ungranted-project");
    let denied = fetch_action(cx, &server, &cert, &machine, foreign_list);
    assert_eq!(denied["error"], "unauthorized", "项目级列表仍须准确授权");
    assert!(denied.get("result").is_none(), "越权不得泄漏历史摘要");
    let mut invalid_list = project_list.clone();
    invalid_list["node_id"] = json!(0);
    assert_eq!(
        fetch_action(cx, &server, &cert, &machine, invalid_list)["error"],
        "invalid_request",
        "非法卡片ID不能扩大为全部成果"
    );

    let started = fetch_action(cx, &server, &cert, &machine, accept.clone());
    assert_eq!(
        started["result"]["status"], "receipt",
        "验收需走正式持久化操作入口: {started}"
    );
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"operation_id":"accept-history"});
    //100×20ms=2s，仅等待本地SQLite提交，不能把accepted当作成功。
    let mut receipt = started;
    for _ in 0..100 {
        if receipt["result"]["receipt"]["state"] != "accepted" {
            break;
        }
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
        receipt = fetch_action(cx, &server, &cert, &machine, query.clone());
    }
    assert_eq!(
        receipt["result"]["receipt"]["state"], "succeeded",
        "真实验收必须已落盘: {receipt}"
    );
    assert_eq!(
        receipt["result"]["receipt"]["result"]["turn_id"], "history-turn",
        "不能验收到后续任务"
    );
    assert_eq!(
        receipt["result"]["receipt"]["result"]["fingerprint"], delivery.fingerprint,
        "回执绑定实际所见内容"
    );
    let saved = storage::MobileCommandStore::new(store.path().to_path_buf())
        .delivery_acceptance(&workspace, &key)
        .expect("读取")
        .expect("已保存");
    assert_eq!(saved.device_id, "phone-a", "验收人准确保存");
    let mut frozen = action.clone();
    frozen["request"]["delivery_key"] = json!(key);
    let accepted = fetch_action(cx, &server, &cert, &machine, frozen);
    assert_eq!(
        accepted["result"]["review"]["delivery"]["acceptance"]["operation_id"], "accept-history",
        "重新打开成果必须读回真实验收"
    );
    let history = fetch_action(cx, &server, &cert, &machine, project_list.clone());
    let marker = &history["result"]["deliveries"]["items"][0]["acceptance"];
    assert_eq!(
        marker["operation_id"], "accept-history",
        "历史列表必须显示同一次持久化验收"
    );
    assert_eq!(
        marker["fingerprint"], delivery.fingerprint,
        "历史标记绑定所见成果指纹"
    );
    assert_eq!(
        marker["key"]["node_id"], node,
        "删除空卡后仍以原卡片身份查询验收"
    );
    accept["request"]["operation_id"] = json!("duplicate-accept");
    let duplicate = fetch_action(cx, &server, &cert, &machine, accept);
    let mut query_duplicate = query;
    query_duplicate["operation_id"] = json!("duplicate-accept");
    let mut duplicate = duplicate;
    for _ in 0..100 {
        if duplicate["result"]["receipt"]["state"] != "accepted" {
            break;
        }
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
        duplicate = fetch_action(cx, &server, &cert, &machine, query_duplicate.clone());
    }
    assert_eq!(
        duplicate["result"]["receipt"]["error"], "delivery_accepted",
        "另一操作不可覆盖验收: {duplicate}"
    );
    assert!(
        duplicate["result"]["receipt"]["result"].is_null(),
        "重复验收没有新的成功结果"
    );
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(b.path(), window, cx)
        })
    });
    let stale = fetch_action(cx, &server, &cert, &machine, action);
    assert_eq!(stale["error"], "route_expired", "离开项目使旧预览路由失效");
    assert!(stale.get("result").is_none(), "过期窗口不得导出正文");
    let stale_list = fetch_action(cx, &server, &cert, &machine, project_list);
    assert_eq!(
        stale_list["error"], "route_expired",
        "历史摘要也不能绕过旧窗口校验"
    );
    assert!(
        stale_list.get("result").is_none(),
        "失效窗口不得读回验收摘要"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        0,
        "预览不应派发任务或启动Agent"
    );
}
