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
