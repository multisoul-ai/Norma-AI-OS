use storage::{
    MobileCommandStore, MobileTaskBaselineStore, MobileTurnKey, NewMobileCommand, TaskBaseline,
};
use workspace::review::{ContentKind, FileChange, FileVersion, ProjectReview};

fn command() -> NewMobileCommand {
    NewMobileCommand {
        device_id: "phone-a".into(),
        operation_id: "op-a".into(),
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        owner_id: "boot-a".into(),
        request_hash: "a".repeat(64),
    }
}
fn key() -> MobileTurnKey {
    MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    }
}
fn baseline() -> TaskBaseline {
    TaskBaseline::Available {
        review: ProjectReview {
            base_commit: None,
            conflicted: false,
            files: vec![FileChange {
                path: "main.txt".into(),
                status: "??".into(),
                before: None,
                index: None,
                after: Some(FileVersion {
                    kind: ContentKind::Text,
                    executable: false,
                    bytes: b"before task\n".to_vec(),
                }),
                diff: Some("+before task\n".into()),
                staged_diff: Some(String::new()),
            }],
        },
    }
}

/// 由原命令查询已确认的回合，不读取文件正文，也不借用同节点的其他任务。
///
/// 数据构造：phone-a/op-a/project-a/route-a 绑定节点7、run-a/session-a/turn-a；
///   第二条 op-b 在同节点同会话绑定 turn-b，起点文件仍是12字节正文。
/// 执行过程：预留两命令并保存起点 → 重开按第一条操作读取 → 错设备/项目/操作/route → 改坏JSON身份。
/// 预期结果：第一条始终返回 turn-a；未绑定和错误范围无结果；矛盾身份报错，不能降级到当前节点。
#[test]
fn command_turn_lookup_is_durable_and_scope_checked() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let commands = MobileCommandStore::new(path.clone());
    let store = MobileTaskBaselineStore::new(path.clone());
    let first = command();
    commands.reserve(&first).expect("第一条操作");
    assert_eq!(
        store
            .turn_for_command("phone-a", "op-a", "project-a", "route-a")
            .expect("未确认"),
        None,
        "Provider尚未确认时不得猜测回合"
    );
    store.record(&first, &key(), &baseline()).expect("原回合");
    let mut second = first.clone();
    second.operation_id = "op-b".into();
    let mut later = key();
    later.turn_id = "turn-b".into();
    commands.reserve(&second).expect("第二条操作");
    store
        .record(&second, &later, &baseline())
        .expect("同节点另一回合");
    let reopened = MobileTaskBaselineStore::new(path.clone());
    assert_eq!(
        reopened
            .turn_for_command("phone-a", "op-a", "project-a", "route-a")
            .expect("重开"),
        Some(key()),
        "旧任务不能跟随同节点最新turn-b"
    );
    assert_eq!(
        reopened
            .turn_for_command("phone-b", "op-a", "project-a", "route-a")
            .expect("错设备"),
        None,
        "另一设备不能借用操作"
    );
    assert_eq!(
        reopened
            .turn_for_command("phone-a", "missing", "project-a", "route-a")
            .expect("错操作"),
        None,
        "不存在的操作不能回退最新终端"
    );
    assert_eq!(
        reopened
            .turn_for_command("phone-a", "op-a", "project-b", "route-a")
            .expect("错项目"),
        None,
        "另一项目不能取回合身份"
    );
    assert_eq!(
        reopened
            .turn_for_command("phone-a", "op-a", "project-a", "route-b")
            .expect("错路由"),
        None,
        "重开的项目路由不能继承旧运行"
    );
    rusqlite::Connection::open(path).expect("损坏fixture").execute(
        "UPDATE mobile_task_baselines SET data_json=json_set(data_json,'$.key.turn_id','turn-b') WHERE operation_id='op-a'", []
    ).expect("制造索引与内容矛盾");
    assert!(
        reopened
            .turn_for_command("phone-a", "op-a", "project-a", "route-a")
            .is_err(),
        "落盘回合身份矛盾必须拒绝"
    );
}

/// 任务起点与实际回合及原请求唯一绑定，重开可读，跨项目/会话和换结果均不能冒领。
///
/// 数据构造：main.txt起点12字节（before6+空格1+task4+换行1）；phone-a/op-a与Codex节点7/turn-a。
/// 执行过程：预留原命令 → 保存起点 → 重开/重复 → 改字节或回合 → 错项目、错session、未预留命令。
/// 预期结果：原字节可重读；重复返回同一保存记录；冲突均拒绝，既有起点不被覆盖。
#[test]
fn task_baseline_is_durable_scoped_and_immutable() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let request = command();
    let key = key();
    let data = baseline();
    MobileCommandStore::new(path.clone())
        .reserve(&request)
        .expect("原请求");
    let store = MobileTaskBaselineStore::new(path.clone());
    let saved = store.record(&request, &key, &data).expect("绑定真实回合");
    assert_eq!(saved.key, key, "必须记录完整Provider身份");
    assert_eq!(saved.baseline, data, "保存任务开始前的原始数据");
    assert_eq!(
        store.record(&request, &key, &data).expect("重复同结果"),
        saved,
        "重试不能生成另一份起点"
    );
    let reopened = MobileTaskBaselineStore::new(path);
    assert_eq!(
        reopened
            .get("project-a", &key)
            .expect("重开")
            .expect("存在")
            .baseline,
        data,
        "起点不能仅存在进程内存"
    );
    let mut altered = data.clone();
    if let TaskBaseline::Available { review } = &mut altered {
        review.files[0].after.as_mut().expect("正文").bytes = b"altered\n".to_vec();
    }
    assert!(
        store.record(&request, &key, &altered).is_err(),
        "同回合的起点字节不可覆盖"
    );
    let mut other_turn = key.clone();
    other_turn.turn_id = "turn-b".into();
    assert!(
        store.record(&request, &other_turn, &data).is_err(),
        "同一发送操作不能关联另一回合"
    );
    let mut other_session = key.clone();
    other_session.session_id = "session-b".into();
    assert!(
        store
            .get("project-a", &other_session)
            .expect("错误session")
            .is_none(),
        "错会话不能认领起点"
    );
    assert!(
        store.get("project-b", &key).expect("错误项目").is_none(),
        "跨项目读取不返回正文"
    );
    let mut missing = request.clone();
    missing.operation_id = "not-reserved".into();
    assert!(
        store.record(&missing, &other_turn, &data).is_err(),
        "没有真实命令记录不能凭空建任务"
    );
    assert_eq!(
        store
            .get("project-a", &key)
            .expect("最终查询")
            .expect("仍存在")
            .baseline,
        data,
        "所有冲突后原数据必须保留"
    );
}

/// 起点读取失败可以明确记录，但不允许非法路径或超限数据进入交付存储。
///
/// 数据构造：一个Unavailable起点；非法../secret路径；1MiB+1字节超过每版本1MiB上限。
/// 执行过程：分别为三个已预留命令尝试保存 → 读取各自实际回合。
/// 预期结果：失败原因明确可读，不能变成空成果；非法及超限起点没有落库记录。
#[test]
fn baseline_failure_is_explicit_and_invalid_content_is_not_saved() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let commands = MobileCommandStore::new(path.clone());
    let store = MobileTaskBaselineStore::new(path);
    let mut request = command();
    let mut key = key();
    commands.reserve(&request).expect("原请求");
    let unavailable = TaskBaseline::Unavailable {
        reason: workspace::review::ReviewError::TooLarge,
    };
    assert_eq!(
        store
            .record(&request, &key, &unavailable)
            .expect("明确不可用")
            .baseline,
        unavailable,
        "不能将捕获失败冒充干净项目"
    );
    request.operation_id = "bad-path".into();
    key.turn_id = "bad-path-turn".into();
    commands.reserve(&request).expect("路径测试请求");
    let mut bad = baseline();
    if let TaskBaseline::Available { review } = &mut bad {
        review.files[0].path = "../secret".into();
    }
    assert!(
        store.record(&request, &key, &bad).is_err(),
        "非法路径在落库前拒绝"
    );
    assert!(
        store.get("project-a", &key).expect("拒绝后查询").is_none(),
        "拒绝不能留下可验收记录"
    );
    request.operation_id = "too-large".into();
    key.turn_id = "large-turn".into();
    commands.reserve(&request).expect("超限请求");
    let mut large = baseline();
    if let TaskBaseline::Available { review } = &mut large {
        review.files[0].after.as_mut().expect("正文").bytes = vec![b'x'; 1024 * 1024 + 1];
    }
    assert!(
        store.record(&request, &key, &large).is_err(),
        "上限+1必须拒绝而非截断"
    );
    assert!(
        store.get("project-a", &key).expect("超限后查询").is_none(),
        "超限不能保存部分快照"
    );
}

/// 按原派工操作读取成果元数据，不能把节点最新成果或别的设备/项目绑定到旧任务。
/// 数据构造：phone-a/op-a 绑定 turn-a；同节点另有 turn-b，二者都有冻结文件成果。
///   后台写入等待上限 5 秒沿用已有审计提交预算；正文不经元数据接口返回。
/// 执行过程：预留/绑定 → 无成果查询 → 写入两回合成果 → 原操作查询 → 错设备/项目/操作查询。
/// 预期结果：原操作返回 turn-a 的唯一指纹与文件数；没有文件字节；三种错误范围均无结果。
#[test]
fn delivery_metadata_follows_original_operation_not_latest_node() {
    use storage::{
        ApprovalAuditStore, BackgroundApprovalAudit, MobileTaskDeliveryStore, MobileTurnOutcome,
        MobileTurnState, MobileTurnStore, TaskDeliveryContent,
    };
    let dir = tempfile::tempdir().expect("隔离数据");
    let db = dir.path().join("app.sqlite");
    let request = command();
    MobileCommandStore::new(db.clone())
        .reserve(&request)
        .expect("预留");
    MobileTaskBaselineStore::new(db.clone())
        .record(&request, &key(), &baseline())
        .expect("原回合绑定");
    let deliveries = MobileTaskDeliveryStore::new(db.clone());
    assert!(
        deliveries
            .for_command("phone-a", "op-a", "project-a")
            .expect("未完成")
            .is_none(),
        "没有成果不能冒充空文件交付"
    );
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db, "project-a".into())),
    )
    .expect("终态 writer");
    let mut original = None;
    for turn in ["turn-a", "turn-b"] {
        let mut identity = key();
        identity.turn_id = turn.into();
        writer
            .record_turn(MobileTurnOutcome {
                key: identity.clone(),
                state: MobileTurnState::Completed,
                summary: Some(turn.into()),
            })
            .expect("入队")
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("等待")
            .expect("落盘");
        let TaskBaseline::Available { review } = baseline() else {
            panic!("可用基线")
        };
        let saved = deliveries
            .record(
                "project-a",
                &identity,
                &TaskDeliveryContent::Available { review },
            )
            .expect("冻结成果");
        if turn == "turn-a" {
            original = Some(saved.fingerprint);
        }
    }
    let summary = deliveries
        .for_command("phone-a", "op-a", "project-a")
        .expect("原操作")
        .expect("成果");
    assert_eq!(
        summary.outcome.key.turn_id, "turn-a",
        "不能返回同节点最新 turn-b"
    );
    assert_eq!(
        Some(summary.fingerprint.clone()),
        original,
        "指纹必须来自原回合冻结内容"
    );
    assert_eq!(summary.file_count, 1, "原回合只有 main.txt");
    assert!(
        !serde_json::to_string(&summary)
            .expect("元数据")
            .contains("before task"),
        "轮询不能返回文件正文"
    );
    assert!(
        deliveries
            .for_command("phone-b", "op-a", "project-a")
            .expect("错设备")
            .is_none(),
        "另一设备不能冒领操作成果"
    );
    assert!(
        deliveries
            .for_command("phone-a", "op-a", "project-b")
            .expect("错项目")
            .is_none(),
        "另一项目不能读取成果"
    );
    assert!(
        deliveries
            .for_command("phone-a", "op-b", "project-a")
            .expect("错操作")
            .is_none(),
        "未绑定操作不能借最新节点成果"
    );
}
