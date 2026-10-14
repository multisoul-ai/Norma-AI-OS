use super::test_support::Fixture;
use serde_json::json;

/// 场景：真实MCP工具→SQLite账本→GPUI Adapter，两个已打开区域且当前区域不同。
/// 执行：指定B建卡带任务、重放、查询目录/任务/进度、回复再取消；预期：只有B发生副作用且任务引用同源。
#[test]
fn connector_four_writes_share_adapter_and_durable_receipts() {
    let mut f = Fixture::new();
    let created = f.create("create-1", 1, Some("执行第一项任务"));
    assert_eq!(created["state"], "loaded", "建卡并派工成功: {created}");
    assert_eq!(created["created_by"], "grokbot", "回执来源明确");
    assert!(created["created_at"].as_i64().is_some(), "时间为unix秒");
    assert!(
        created["task_id"].as_str().is_some(),
        "单次建卡派工有任务引用"
    );
    let replay = f.create("create-1", 1, Some("执行第一项任务"));
    assert_eq!(replay["card_id"], created["card_id"], "重放不新建卡片");
    assert_eq!(replay["task_id"], created["task_id"], "重放不新派工");
    assert_eq!(replay["created_at"], created["created_at"], "重放时间不变");
    assert_eq!(replay["replayed"], true, "重放标志");
    f.test.update(|cx| {
        assert_eq!(
            f.sessions[0].read(cx).runtime.document.nodes().len(),
            0,
            "A区域没有节点副作用"
        );
        assert_eq!(
            f.sessions[1].read(cx).runtime.document.nodes().len(),
            1,
            "仅B出现一张卡片"
        );
    });
    let directory = f.call("directory", json!({}));
    let card = directory["workspaces"][0]["cards"][0].clone();
    assert_eq!(card["card_id"], created["card_id"], "目录与回执身份一致");
    assert_eq!(
        card["card_revision"], created["card_revision"],
        "同Provider绑定revision一致"
    );
    assert_eq!(card["created_by"], "grokbot", "来源关联账本");
    let tasks = f.call("list_tasks", json!({}));
    assert_eq!(
        tasks["tasks"].as_array().expect("任务列表").len(),
        1,
        "建卡task仅派工一次"
    );
    assert_eq!(
        tasks["tasks"][0]["task_id"], created["task_id"],
        "V-4列表与Adapter任务同源"
    );
    let progress = f.call("get_task_progress", json!({"operation_id":"create-1"}));
    assert_eq!(progress["task_id"], created["task_id"], "原操作可核对任务");
    assert_ne!(progress["state"], "done", "接收/提交不等于完成");
    let message = json!({"operation_id":"reply-1","card_id":card["card_id"],"card_revision":card["card_revision"],"message":"补充说明"});
    let reply = f.call("reply_agent", message.clone());
    assert!(reply["reply_id"].as_str().is_some(), "真实会话回复编号");
    let repeated = f.call("reply_agent", message);
    assert_eq!(repeated["reply_id"], reply["reply_id"], "回复不重复发送");
    assert_eq!(
        f.call("list_tasks", json!({}))["tasks"]
            .as_array()
            .expect("任务")
            .len(),
        1,
        "回复不创建任务"
    );
    let cancel = f.call(
        "cancel_task",
        json!({"operation_id":"cancel-1","task_id":created["task_id"]}),
    );
    assert_eq!(cancel["state"], "cancelled", "取消未提交任务");
    let again = f.call(
        "cancel_task",
        json!({"operation_id":"cancel-1","task_id":created["task_id"]}),
    );
    assert_eq!(
        again["requested_at"], cancel["requested_at"],
        "取消重放时间不变"
    );
    let terminal = f.call(
        "cancel_task",
        json!({"operation_id":"cancel-2","task_id":created["task_id"]}),
    );
    assert_eq!(
        terminal["state"], "already_terminal",
        "终态取消没有中断副作用"
    );
    assert_eq!(terminal["terminal_state"], "cancelled", "明确终态");
}

/// 场景：预留后的revision拒绝不可因条件变化重执行；省略allow_queue与true等价。
/// 执行：旧revision两次、跨工具冲突、合法新ID派工与显式默认重放。
/// 预期：原拒绝重放不输入；新ID只派一次；规范化参数和工具名均进入指纹。
#[test]
fn connector_rejections_freeze_and_defaults_normalize() {
    let mut f = Fixture::new();
    let card = f.create("card", 0, None);
    let stale = json!({"operation_id":"stale","card_id":card["card_id"],"card_revision":"old","task":"不能投递"});
    assert_eq!(
        f.call("dispatch_task", stale.clone())["error"]["code"],
        "target_changed",
        "执行期拒绝"
    );
    assert_eq!(
        f.call("dispatch_task", stale)["replayed"],
        true,
        "拒绝持久化重放"
    );
    assert_eq!(f.call("reply_agent",json!({"operation_id":"stale","card_id":card["card_id"],"card_revision":"old","message":"不能投递"}))["error"]["code"],"operation_conflict","跨工具指纹冲突");
    let input = json!({"operation_id":"dispatch","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"合法派工"});
    let first = f.call("dispatch_task", input.clone());
    assert!(first["task_id"].as_str().is_some(), "合法新ID派工");
    assert_ne!(first["state"], "done", "派工不是完成");
    let mut explicit = input;
    explicit["allow_queue"] = json!(true);
    let second = f.call("dispatch_task", explicit);
    assert_eq!(second["task_id"], first["task_id"], "默认值规范化");
    assert_eq!(second["accepted_at"], first["accepted_at"], "派工时间不变");
    assert_eq!(second["replayed"], true, "显式默认是同请求");
    let rows = f.call("list_tasks", json!({}));
    assert_eq!(
        rows["tasks"].as_array().expect("任务").len(),
        1,
        "旧revision与重放零额外任务"
    );
}

/// 场景：未知字段、错误profile、缺area、不存在area、关闭区域，在任何预留前拒绝。
/// 执行：逐项调用同一operation_id，然后用该ID成功建卡；预期：坏输入不占键，不启动其它区域。
#[test]
fn connector_preflight_errors_do_not_reserve_or_load() {
    let mut f = Fixture::new();
    let base = json!({"operation_id":"free","workspace_id":f.workspace,"area_id":f.areas[0],"profile":"codex"});
    for profile in ["opencode", "cursor", "grok"] {
        let mut args = base.clone();
        args["profile"] = json!(profile);
        assert_eq!(
            f.call("create_card", args)["error"]["code"],
            "profile_unavailable",
            "只允许两个profile"
        );
    }
    let mut missing = base.clone();
    missing.as_object_mut().expect("对象").remove("area_id");
    assert_eq!(
        f.call("create_card", missing)["error"]["code"],
        "invalid_argument",
        "区域必填"
    );
    let mut unknown = base.clone();
    unknown["area_id"] = json!("missing");
    assert_eq!(
        f.call("create_card", unknown)["error"]["code"],
        "area_not_found",
        "区域不替换为焦点"
    );
    let mut forbidden = base.clone();
    forbidden["start"] = json!(false);
    assert_eq!(
        f.call("create_card", forbidden)["error"]["code"],
        "invalid_argument",
        "不开放start或旧参数"
    );
    f.test.update(|cx| {
        f.host.update(cx, |app, _| {
            app.desktop_workspace
                .sessions
                .remove(&(f.workspace.clone(), f.areas[0].clone()));
        })
    });
    assert_eq!(
        f.call("create_card", base.clone())["error"]["code"],
        "workspace_closed",
        "关闭区域不打开"
    );
    f.test.update(|cx| {
        f.host.update(cx, |app, _| {
            app.desktop_workspace.sessions.insert(
                (f.workspace.clone(), f.areas[0].clone()),
                f.sessions[0].clone(),
            );
        })
    });
    let made = f.call("create_card", base);
    assert_eq!(made["state"], "loaded", "之前错误没有占键");
    assert_eq!(
        f.create("next", 0, None)["state"],
        "loaded",
        "没有数量限制或二次确认"
    );
}

/// 场景：轮换后重放建卡，与吊销后新的命名空间；关闭旧窗口不代表空目录。
/// 执行：生成随机Token、轮换、吊销再生成，并使用旧operation_id；预期：轮换同回执，吊销新建。
#[test]
fn connector_epoch_controls_creation_and_cancel_ownership() {
    let mut f = Fixture::new();
    let first = f.create("same", 0, Some("首任务"));
    let rotated = f.auth.generate().expect("随机轮换");
    f.identity = f.auth.authenticate(Some(&rotated)).expect("新Token");
    assert_eq!(
        f.create("same", 0, Some("首任务"))["card_id"],
        first["card_id"],
        "轮换共用账本"
    );
    f.auth.revoke().expect("吊销");
    let fresh = f.auth.generate().expect("新授权");
    f.identity = f.auth.authenticate(Some(&fresh)).expect("新epoch");
    assert_eq!(
        f.call(
            "cancel_task",
            json!({"operation_id":"cancel-old","task_id":first["task_id"]})
        )["error"]["code"],
        "not_connector_task",
        "新epoch不能取消旧授权任务"
    );
    let second = f.create("same", 0, Some("首任务"));
    assert_ne!(
        second["card_id"], first["card_id"],
        "新epoch不返回旧建卡回执"
    );
    assert_eq!(second["replayed"], false, "新命名空间实际执行一次");
}

/// 场景：卡片改名保留身份；删除后同node_id换定义，旧目标不得被自动替代。
/// 执行：真实目录→修改定义展示名→旧card_id派工，再替换节点定义并重试。
/// 预期：改名可派，重建target_changed且无额外任务；未加载回复不启动PTY。
#[test]
fn connector_card_identity_survives_rename_and_rejects_recreation() {
    let mut f = Fixture::new();
    let original = f.create("original", 0, None);
    let identity = f
        .repo
        .signer
        .verify(original["card_id"].as_str().expect("身份"))
        .expect("签名");
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            assert!(
                app.agent_definitions
                    .edit_definition(identity.definition_id),
                "编辑原定义"
            );
            app.agent_definitions.set_draft_name("Renamed");
            assert!(app.agent_definitions.save_draft(), "保存新展示名");
        })
    });
    let directory = f.call("directory", json!({}));
    assert_eq!(
        directory["workspaces"][0]["cards"][0]["card_id"], original["card_id"],
        "改名不改变身份"
    );
    assert_eq!(
        directory["workspaces"][0]["cards"][0]["card_revision"], original["card_revision"],
        "改名不改变持久目标指纹"
    );
    let sent = f.call("dispatch_task",json!({"operation_id":"after-rename","card_id":original["card_id"],"card_revision":original["card_revision"],"task":"rename work"}));
    assert!(sent["task_id"].as_str().is_some(), "旧card_id仍指向原卡片");
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            let bounds = app
                .runtime
                .document
                .node(identity.node_id)
                .expect("节点")
                .bounds;
            app.runtime.remove_node(identity.node_id);
            assert!(
                app.agent_definitions
                    .delete_definition(identity.definition_id),
                "重建前移除原定义，使同名可重新创建"
            );
            let definition = app
                .agent_definitions
                .create_definition(Some("Renamed".into()), canvas::AgentProfile::Codex)
                .expect("新定义");
            let mut snapshot = app.runtime.collect_canvas_snapshot();
            snapshot.nodes.push(canvas::CanvasNodeSnapshot {
                id: identity.node_id,
                kind: canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                    profile: canvas::AgentProfile::Codex,
                    definition_id: Some(definition.id),
                    alias: None,
                    mobile_control: false,
                }),
                bounds,
                z_index: 0,
            });
            app.runtime.restore_canvas_snapshot(snapshot);
            app.runtime
                .start_native_agent_terminal(identity.node_id, 80, 24)
                .expect("测试PTY重建");
        })
    });
    assert_eq!(f.call("dispatch_task",json!({"operation_id":"after-recreate","card_id":original["card_id"],"card_revision":original["card_revision"],"task":"不得替代"}))["error"]["code"],"target_changed","同node_id换身份拒绝");
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime.remove_node(identity.node_id);
        })
    });
    assert_eq!(f.call("reply_agent",json!({"operation_id":"unloaded","card_id":original["card_id"],"card_revision":original["card_revision"],"message":"不能自动加载"}))["error"]["code"],"card_not_loaded","未加载回复不打开卡片");
    f.test.update(|cx| {
        assert_eq!(
            f.sessions[0].read(cx).runtime.live_len(),
            0,
            "回复没有启动PTY"
        )
    });
}

/// 场景：审批、忙碌和无Hook的卡片分别拒绝派工；拒绝后重放保留原错误。
/// 执行：注入真实Runtime状态、调用工具，再解除状态并重放；预期：稳定码、零新任务、不force。
#[test]
fn connector_dispatch_guards_approval_busy_and_untracked() {
    let mut f = Fixture::new();
    let card = f.create("card", 0, None);
    let identity = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("身份");
    let update = |to| runtime::AgentTaskUpdate {
        node_id: identity.node_id,
        profile: canvas::AgentProfile::Codex,
        provider_event: "PermissionRequest".into(),
        to,
        task_title: None,
        session_id: None,
    };
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .apply_agent_task_update(update(runtime::AgentTaskState::WaitingApproval {
                    tool_name: Some("Bash".into()),
                }));
        })
    });
    let blocked = json!({"operation_id":"approval","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"blocked"});
    assert_eq!(
        f.call("dispatch_task", blocked.clone())["error"]["code"],
        "approval_pending",
        "不越过审批"
    );
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .apply_agent_task_update(update(runtime::AgentTaskState::Thinking));
        })
    });
    let busy = json!({"operation_id":"busy","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"busy","allow_queue":false});
    assert_eq!(
        f.call("dispatch_task", busy)["error"]["code"],
        "card_busy",
        "不允许排队时忙碌拒绝"
    );
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .apply_agent_task_update(update(runtime::AgentTaskState::Ready));
            app.disable_agent_hook_for_test();
        })
    });
    assert_eq!(
        f.call("dispatch_task", blocked)["error"]["code"],
        "approval_pending",
        "原拒绝不会因审批解除而重执行"
    );
    assert_eq!(f.call("dispatch_task",json!({"operation_id":"untracked","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"不强制"}))["error"]["code"],"untracked_requires_confirmation","来源恒为非force");
    f.test.update(|cx| {
        assert!(
            f.sessions[0]
                .read(cx)
                .agent_task_coordinator
                .as_ref()
                .is_none_or(|c| c.tasks().is_empty()),
            "所有执行期拒绝零任务"
        )
    });
}

/// 场景：非连接器任务默认拒绝取消；用户保存放开设置后允许，手机权限模型不参与。
/// 执行：语音Adapter建卡派工、MCP取消、修改持久设置、MCP再次取消。
/// 预期：默认not_connector_task，无副作用；显式设置后只取消目标。
#[test]
fn connector_cancel_other_tasks_requires_persisted_setting() {
    let mut f = Fixture::new();
    let task = f.test.update(|cx| {
        f.sessions[0].update(cx, |app, cx| {
            let result = crate::app::execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                agent::AdapterCommand::CreateAgent {
                    profile: norma::protocol::AgentProfile::Codex,
                    task: Some("桌面任务".into()),
                },
                agent::AgentInputSource::Voice,
            )
            .expect("桌面派工");
            app.agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .tasks()
                .into_iter()
                .find(|t| Some(t.task_id) == result.task_id)
                .expect("任务")
                .task_ref
        })
    });
    assert_eq!(
        f.call(
            "cancel_task",
            json!({"operation_id":"denied","task_id":task})
        )["error"]["code"],
        "not_connector_task",
        "默认所有权边界"
    );
    let store = storage::connector_store::ConnectorStore::new(f.repo.database.clone());
    let mut config = store.config().expect("设置");
    config.allow_cancel_other_tasks = true;
    store.save_config(&config).expect("明确放开");
    assert_eq!(
        f.call(
            "cancel_task",
            json!({"operation_id":"allowed","task_id":task})
        )["state"],
        "cancelled",
        "放开后正常走Adapter取消"
    );
}
