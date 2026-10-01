use serde_json::json;
use storage::{
    MobileCommandReservation, MobileCommandStore, NewMobileCommand, NewThreadMessage,
    ThreadCommandBinding, ThreadMessageStore,
};

/// 线程派工复用命令账本：只预留一次，路由变化不改变身份，来源与执行回执必须同属原线程。
///
/// 数据构造：1 条已送达来源、1 个 Agent 派工；64 位 a/b 表示不同正文指纹。
///   route-a → route-b、boot-a → boot-b 模拟重开项目和 Mac；回执节点固定 7。
/// 执行过程：绑定来源并预留 → 换路由/进程重试 → 保存真实派工回执 → 查原线程/另一线程 → 改正文。
/// 预期结果：只一次 New；重启未完成为 unknown；完成后仍返回节点 7；错误线程查不到，改正文冲突。
#[test]
fn thread_command_keeps_identity_across_routes_and_restarts() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    source(&path);
    let store = MobileCommandStore::new(path.clone());
    let request = command();
    let binding = binding();
    assert!(
        matches!(
            store
                .reserve_thread_command(&request, &binding)
                .expect("首次预留"),
            MobileCommandReservation::New
        ),
        "首次派工必须获一次执行权"
    );
    let mut retry = request.clone();
    retry.route_id = "route-b".into();
    retry.owner_id = "boot-b".into();
    let MobileCommandReservation::Existing(unknown) = store
        .reserve_thread_command(&retry, &binding)
        .expect("重试")
    else {
        panic!("换路由和启动实例只能取原回执");
    };
    assert_eq!(
        unknown.state, "outcome_unknown",
        "重启不能重做结果未知的副作用"
    );
    assert_eq!(unknown.route_id, "route-a", "保留原执行 route 的事实");
    store
        .finish(&request, Ok(json!({"node_id":7,"task_id":3})))
        .expect("原执行结果");
    let reopened = MobileCommandStore::new(path);
    let saved = reopened
        .thread_command("phone", "mac", "workspace", "thread", "task", "boot-b")
        .expect("查询")
        .expect("持久绑定");
    assert_eq!(
        saved.receipt.result,
        Some(json!({"node_id":7,"task_id":3})),
        "原线程必须取得原任务"
    );
    assert_eq!(
        saved.binding, binding,
        "任务正文、来源与归属不能在恢复时重建"
    );
    assert!(
        reopened
            .thread_command(
                "phone",
                "mac",
                "workspace",
                "other-thread",
                "task",
                "boot-b"
            )
            .expect("隔离查询")
            .is_none(),
        "其他线程不能读取此任务"
    );
    retry.request_hash = "b".repeat(64);
    assert!(
        matches!(
            reopened
                .reserve_thread_command(&retry, &binding)
                .expect("冲突检查"),
            MobileCommandReservation::Conflict
        ),
        "同操作换正文不得派工"
    );
    let mut moved = binding.clone();
    moved.thread_id = "other-thread".into();
    assert!(
        matches!(
            reopened
                .reserve_thread_command(&request, &moved)
                .expect("归属检查"),
            MobileCommandReservation::Conflict
        ),
        "同操作不能移动到另一线程"
    );
}

/// 来源缺失或线程绑定写入失败时，不能遗留已经占用执行身份的命令；旧协议操作不得被挪作线程任务。
/// 数据构造：1 条有效来源；binding INSERT trigger 仅注入本测试的磁盘写失败，预期命令数为 0。
/// 执行过程：错来源预留 → 注入失败 → 用命令公开查询确认无记录 → 恢复后预留 → 尝试接管旧命令。
/// 预期结果：失败不占操作 ID；恢复只一次 New；已存在的旧 command 保留原用途，不能被绑定到线程。
#[test]
fn thread_binding_failure_does_not_reserve_or_hijack_a_command() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    source(&path);
    let store = MobileCommandStore::new(path.clone());
    let request = command();
    let mut missing = binding();
    missing.source_message_id = "missing".into();
    assert!(
        store.reserve_thread_command(&request, &missing).is_err(),
        "未送达的来源不得派工"
    );
    let database = rusqlite::Connection::open(path).expect("仅用于失败注入");
    database
        .execute_batch(
            "CREATE TRIGGER fail_thread_binding BEFORE INSERT ON mobile_thread_commands
        BEGIN SELECT RAISE(ABORT,'test failure'); END;",
        )
        .expect("写失败");
    assert!(
        store.reserve_thread_command(&request, &binding()).is_err(),
        "绑定写失败不得获得执行权"
    );
    assert!(
        store
            .get("phone", "task", "project", "boot-a")
            .expect("查询操作")
            .is_none(),
        "绑定失败必须连同命令一起回滚"
    );
    database
        .execute_batch("DROP TRIGGER fail_thread_binding;")
        .expect("恢复测试写入");
    assert!(
        matches!(
            store
                .reserve_thread_command(&request, &binding())
                .expect("恢复后重试"),
            MobileCommandReservation::New
        ),
        "失败后原操作仍可准确预留一次"
    );
    let mut legacy = request;
    legacy.operation_id = "legacy".into();
    store.reserve(&legacy).expect("旧命令");
    assert!(
        matches!(
            store
                .reserve_thread_command(&legacy, &binding())
                .expect("旧协议冲突"),
            MobileCommandReservation::Conflict
        ),
        "不能把其他入口的执行回执绑定到线程"
    );
    assert!(
        store
            .thread_command("phone", "mac", "workspace", "thread", "legacy", "boot-a")
            .expect("查绑定")
            .is_none(),
        "旧命令冲突不得遗留线程绑定"
    );
}

/// 已执行任务的同步事件跨重启稳定，旧运行状态不能覆盖终态，相同数字 ID 的新任务不能冒领。
///
/// 数据构造：phone/task 已派给 boot-a:3、节点 7、Agent 2；结果正文“导航已修复”。
///   两个快照分别为 submitted 和 done，故预期仅产生 2 个递增序号。
/// 执行过程：预留来源和任务 → 尝试在接单前写事件 → 接单 → 保存运行/完成 → 重开/重复 → 错引用/迟到状态。
/// 预期结果：未接单拒绝；重复重用事件；done 不退回 submitted；boot-b:3 和修改后的最终正文不得保存。
#[test]
fn thread_execution_events_are_durable_bound_and_monotonic() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    source(&path);
    let store = MobileCommandStore::new(path.clone());
    let request = command();
    store
        .reserve_thread_command(&request, &binding())
        .expect("预留任务");
    let before = store
        .thread_command("phone", "mac", "workspace", "thread", "task", "boot-a")
        .expect("查询")
        .expect("任务");
    let running: storage::ThreadTaskExecution = serde_json::from_value(json!({
        "task_ref":"boot-a:3", "node_id":7, "agent_definition_id":2,
        "state":"submitted", "tracking":"managed", "error":null, "result":null, "delivery":null
    }))
    .expect("执行快照");
    assert!(
        store.record_thread_execution(&before, &running).is_err(),
        "未派工不能凭空产生执行事件"
    );
    store
        .finish(
            &request,
            Ok(json!({"status":"accepted", "node_id":7,"task_id":3,"task_ref":"boot-a:3"})),
        )
        .expect("持久派工回执");
    let bound = store
        .thread_command("phone", "mac", "workspace", "thread", "task", "boot-a")
        .expect("查询")
        .expect("已派工");
    let first = store
        .record_thread_execution(&bound, &running)
        .expect("运行事件");
    let mut done = running.clone();
    done.state = "done".into();
    done.result = Some(storage::StoredAgentDispatchResult {
        text: Some("导航已修复".into()),
        truncated: false,
    });
    let finished = store
        .record_thread_execution(&bound, &done)
        .expect("完成事件");
    assert!(
        finished.sequence > first.sequence,
        "状态变化必须推进持久游标"
    );
    assert_ne!(
        finished.event_id, first.event_id,
        "状态变化必须有新的事件身份"
    );
    let reopened = MobileCommandStore::new(path);
    assert_eq!(
        reopened
            .record_thread_execution(&bound, &done)
            .expect("重复完成"),
        finished,
        "重复查询不能生成第二条回复事件"
    );
    assert_eq!(
        reopened
            .record_thread_execution(&bound, &running)
            .expect("迟到运行"),
        finished,
        "完成不能退回运行"
    );
    let mut wrong = done.clone();
    wrong.task_ref = "boot-b:3".into();
    assert!(
        reopened.record_thread_execution(&bound, &wrong).is_err(),
        "相同数字 ID 的新 runtime 不能冒领原任务"
    );
    done.result.as_mut().expect("结果").text = Some("另一任务结果".into());
    assert!(
        reopened.record_thread_execution(&bound, &done).is_err(),
        "已经返回的最终回复不能被静默替换"
    );
    assert_eq!(
        reopened
            .thread_execution(&bound)
            .expect("恢复")
            .expect("事件"),
        finished,
        "所有冲突后原回复与游标必须保留"
    );
}

/// 成果晚到可补同一完成任务，但同项目同节点的另一回合成果仍必须拒绝。
/// 数据构造：phone/task 绑定节点7的turn-a；另有同节点turn-b；两者均冻结0文件的成果。
///   审计 writer 等待上限沿用5秒；原完成事件1条，补成果后新增1条。
/// 执行过程：完成任务 → 保存两个回合成果 → 尝试冒领turn-b → 补turn-a → 重放无成果旧快照。
/// 预期结果：turn-b不落事件；turn-a推进序号且不改回复；旧快照不能移除已到达成果。
#[test]
fn thread_execution_only_attaches_the_original_commands_delivery() {
    use storage::{
        ApprovalAuditStore, BackgroundApprovalAudit, MobileTaskBaselineStore,
        MobileTaskDeliveryStore, MobileTurnKey, MobileTurnOutcome, MobileTurnState,
        MobileTurnStore, TaskBaseline, TaskDeliveryContent,
    };
    let dir = tempfile::tempdir().expect("隔离目录");
    let db = dir.path().join("app.sqlite");
    source(&db);
    let store = MobileCommandStore::new(db.clone());
    let request = command();
    store
        .reserve_thread_command(&request, &binding())
        .expect("预留");
    store
        .finish(
            &request,
            Ok(json!({"status":"accepted","task_id":3,"task_ref":"boot-a:3","node_id":7})),
        )
        .expect("接单");
    let bound = store
        .thread_command("phone", "mac", "workspace", "thread", "task", "boot-a")
        .expect("查询")
        .expect("原绑定");
    let done: storage::ThreadTaskExecution =
        serde_json::from_value(json!({"task_ref":"boot-a:3","node_id":7,
        "agent_definition_id":2,"state":"done","tracking":"managed","error":null,
        "result":{"text":"修复完成","truncated":false},"delivery":null}))
        .expect("完成快照");
    let first = store
        .record_thread_execution(&bound, &done)
        .expect("回复先到");
    let mut key = MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    let review = workspace::review::ProjectReview {
        base_commit: None,
        conflicted: false,
        files: vec![],
    };
    MobileTaskBaselineStore::new(db.clone())
        .record(
            &request,
            &key,
            &TaskBaseline::Available {
                review: review.clone(),
            },
        )
        .expect("原操作绑定");
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db.clone(), "project".into())),
    )
    .expect("writer");
    let deliveries = MobileTaskDeliveryStore::new(db);
    for turn in ["turn-a", "turn-b"] {
        key.turn_id = turn.into();
        writer
            .record_turn(MobileTurnOutcome {
                key: key.clone(),
                state: MobileTurnState::Completed,
                summary: None,
            })
            .expect("终态")
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("等待")
            .expect("持久化");
        deliveries
            .record(
                "project",
                &key,
                &TaskDeliveryContent::Available {
                    review: review.clone(),
                },
            )
            .expect("冻结成果");
    }
    let mut late = done.clone();
    late.delivery = deliveries
        .list("project", Some(7), None)
        .expect("成果目录")
        .into_iter()
        .next();
    assert_eq!(
        late.delivery
            .as_ref()
            .expect("最新成果")
            .outcome
            .key
            .turn_id,
        "turn-b",
        "负例必须确实指向别的回合"
    );
    assert!(
        store.record_thread_execution(&bound, &late).is_err(),
        "同节点另一回合成果不能挂到原任务"
    );
    late.delivery = deliveries
        .for_command("phone", "task", "project")
        .expect("原成果");
    let updated = store
        .record_thread_execution(&bound, &late)
        .expect("原成果晚到");
    assert!(updated.sequence > first.sequence, "成果晚到应推进游标");
    assert_eq!(
        updated.execution.result, done.result,
        "成果补齐不能替换原回复"
    );
    assert_eq!(
        updated
            .execution
            .delivery
            .as_ref()
            .expect("原成果")
            .outcome
            .key
            .turn_id,
        "turn-a",
        "只绑定原回合"
    );
    assert_eq!(
        store
            .record_thread_execution(&bound, &done)
            .expect("迟到旧快照"),
        updated,
        "旧快照不能清掉已同步成果"
    );
}

/// 查询已移除项目的任务不能创建空项目数据库，也不能影响之后恢复原数据库。
/// 数据构造：隔离目录下不存在的 .multisoul/project.test.sqlite；查询完整引用 boot-a:3。
/// 执行过程：公开持久引用查询 → 检查返回值与文件路径。
/// 预期结果：没有任务；不产生空 SQLite 文件或项目元数据目录。
#[test]
fn missing_execution_lookup_does_not_create_a_project_database() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let path = directory.path().join(".multisoul/project.test.sqlite");
    let store = storage::AgentDispatchTaskStore::new(path.clone());
    assert!(
        store
            .get_by_task_ref("boot-a:3")
            .expect("缺失任务")
            .is_none(),
        "缺失库不能返回执行成功"
    );
    assert!(!path.exists(), "只读任务恢复不能创建空项目数据库");
    assert!(
        !directory.path().join(".multisoul").exists(),
        "只读查询不能新建项目目录"
    );
}

fn command() -> NewMobileCommand {
    NewMobileCommand {
        device_id: "phone".into(),
        operation_id: "task".into(),
        workspace_id: "project".into(),
        route_id: "route-a".into(),
        owner_id: "boot-a".into(),
        request_hash: "a".repeat(64),
    }
}

fn binding() -> ThreadCommandBinding {
    ThreadCommandBinding {
        source_mac_id: "mac".into(),
        collaboration_workspace_id: "workspace".into(),
        thread_id: "thread".into(),
        source_operation_id: "message-op".into(),
        source_message_id: "message".into(),
        payload: json!({"agent_definition_id":2,"task":"修复首屏"}).to_string(),
    }
}

fn source(path: &std::path::Path) {
    ThreadMessageStore::new(path.into())
        .receive(&NewThreadMessage {
            device_id: "phone".into(),
            source_mac_id: "mac".into(),
            collaboration_workspace_id: "workspace".into(),
            thread_id: "thread".into(),
            operation_id: "message-op".into(),
            message_id: "message".into(),
            origin_device_id: "origin".into(),
            fingerprint: "a".repeat(64),
            payload: "{}".into(),
        })
        .expect("来源先持久送达");
}
