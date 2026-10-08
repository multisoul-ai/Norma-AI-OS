use super::*;

struct InboxFixture {
    server: super::super::MobileServer,
    store: storage::AppStore,
    space: String,
    area: String,
    project: String,
    captures: async_channel::Receiver<super::super::LiveWorkspaceRequest>,
    commands: async_channel::Receiver<super::super::LiveCommandRequest>,
    _dir: tempfile::TempDir,
}
impl InboxFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("隔离目录");
        let store =
            storage::AppStore::initialize(&storage::DatabasePaths::isolated_test(dir.path()))
                .expect("隔离库");
        let catalog = store.desktop_workspace_store();
        let space = catalog.create("收件箱测试空间").expect("空间");
        let root = dir.path().join("project");
        std::fs::create_dir(&root).expect("项目根目录");
        let area = catalog.add_project(&space.id, &root).expect("区域");
        let project = store
            .workspace_identity_store()
            .resolve_or_create(&area.root, "project")
            .expect("项目身份")
            .workspace_id;
        store
            .mobile_access_store()
            .register_all_projects(
                "phone",
                "手机",
                &"a".repeat(64),
                storage::MobilePermission::Control,
            )
            .expect("控制授权");
        // Linux has no Keychain; the API fixture supplies an isolated generated server identity.
        let access = super::super::MobileAccess {
            store: store.clone(),
            identity: norma::mobile::ServerIdentity::generate().expect("身份"),
            pairing: None,
        };
        let (send_capture, captures) = async_channel::bounded(2);
        let (send_command, commands) = async_channel::bounded(2);
        let server = super::super::MobileServer::start_with_mobile_sources(
            access,
            vec!["127.0.0.1:0".parse().expect("回环地址")],
            super::super::MobileSources {
                workspaces: Some(send_capture),
                commands: Some(send_command),
                ..Default::default()
            },
        )
        .expect("本地服务");
        Self {
            server,
            store,
            space: space.id,
            area: area.id,
            project,
            captures,
            commands,
            _dir: dir,
        }
    }
    fn wire(&self, kind: &str) -> Value {
        json!({"desktop_workspace_id":self.space,"kind":kind,"operation_id":format!("op-{kind}"),
            "target":{"area_id":self.area,"node_id":1,"profile":"Claude","run_id":"run","session_id":"session","turn_id":"turn"}})
    }
    fn call(&self, wire: Value) -> Result<Value, &'static str> {
        let request = serde_json::from_value(wire).expect("typed wire");
        let machine = self.server.shared.access().expect("访问锁").machine_id();
        handle(
            &self.server.shared,
            &machine,
            "phone",
            &"a".repeat(64),
            &request,
        )
    }
    fn captured(&self, state: &str, turn: &str) -> Value {
        json!([{"route_id":"route","approvals":[],"project":{"area_id":self.area,"project_id":self.project,"name":"项目","root":"",
            "agents":[{"definition_id":1,"name":"Agent","profile":"Claude","card":{"node_id":1,"profile":"Claude","title":"Agent",
            "state":state,"mobile_control":true,"run_id":"run","session_id":"session","turn_id":turn}}],"definitions":[]}}])
    }
    fn capture_once(&self, state: &str, turn: &str) -> std::thread::JoinHandle<()> {
        let receiver = self.captures.clone();
        let captured = self.captured(state, turn);
        std::thread::spawn(move || {
            let call = receiver.recv_blocking().expect("捕获请求");
            assert!(
                matches!(call.action, LiveWorkspaceAction::ActivityCapture),
                "只允许现有活动捕获"
            );
            call.reply.send(Ok(captured)).expect("回复捕获");
        })
    }
    fn delivery(&self) -> String {
        let wire = self.wire("return_for_changes");
        let target: WorkspaceActivityTarget =
            serde_json::from_value(wire["target"].clone()).expect("目标");
        let key = key(&target);
        let scope = scope(&self.space, &self.area);
        let writer = storage::BackgroundApprovalAudit::new(
            storage::ApprovalAuditStore::new(self._dir.path().join("audit.sqlite"))
                .with_turn_store(storage::MobileTurnStore::for_workspace(
                    self.store.path().to_owned(),
                    scope.clone(),
                )),
        )
        .expect("审计writer");
        writer
            .record_turn(storage::MobileTurnOutcome {
                key: key.clone(),
                state: storage::MobileTurnState::Completed,
                summary: None,
            })
            .expect("入队")
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("5秒本地提交预算")
            .expect("提交终态");
        storage::MobileTaskDeliveryStore::new(self.store.path().to_owned())
            .record(
                &scope,
                &key,
                &storage::TaskDeliveryContent::Available {
                    review: workspace::review::ProjectReview {
                        base_commit: None,
                        files: vec![],
                        conflicted: false,
                    },
                },
            )
            .expect("冻结成果")
            .fingerprint
    }
}

/// 场景：放弃只写项目级忽略记录，同operation重放及Receipt查询返回相同结果。
/// 数据构造：一个failed卡片和Control手机；只提供一次真实捕获。
/// 执行过程：放弃→检查键/成功回执→重复放弃→查询Receipt→检查无新捕获/Provider命令。
/// 预期结果：忽略键采用项目+事项；重复无需活任务；不改变/重新发送Mac任务。
#[test]
fn inbox_dismiss_is_durable_idempotent_and_never_sends_task_input() {
    let f = InboxFixture::new();
    let wire = f.wire("dismiss");
    let capture = f.capture_once("failed", "turn");
    let first = f.call(wire.clone()).expect("放弃");
    capture.join().expect("捕获结束");
    assert_eq!(
        first["workspace_activity"]["receipt"]["state"], "succeeded",
        "放弃须返回成功回执"
    );
    let target: WorkspaceActivityTarget =
        serde_json::from_value(wire["target"].clone()).expect("目标");
    let keys = storage::MobileCommandStore::new(f.store.path().to_owned())
        .inbox_dismissed_keys(&f.project)
        .expect("项目级忽略键");
    assert_eq!(keys.len(), 1, "只能忽略一个事项");
    assert!(
        keys.contains(&inbox_item_key("failed", &f.project, &target)),
        "必须使用项目+事项键"
    );
    assert_eq!(
        f.call(wire).expect("重复放弃"),
        first,
        "相同操作返回相同回执"
    );
    let receipt = f.call(json!({"desktop_workspace_id":f.space,"kind":"receipt","area_id":f.area,"operation_id":"op-dismiss"})).expect("查询结果");
    assert_eq!(receipt, first, "Receipt操作能查询新操作");
    assert!(
        f.captures.is_empty() && f.commands.is_empty(),
        "重复不捕获，放弃不派发Provider输入"
    );
}

/// 场景：重试与退回修改共享命令管线，每个operation只派发一次，即使原卡片后来消失。
/// 数据构造：failed/review各一；原文反馈含首尾空格/换行；模拟Provider确认new-turn。
/// 执行过程：捕获一次→新操作→消费一个Provider命令→存反馈并完成→重放→查询。
/// 预期结果：retry绑定原failed卡；feedback保存完整原文并绑定同session新turn；重放不派发。
#[test]
fn inbox_retry_and_return_for_changes_dispatch_once_and_keep_receipts_after_target_vanishes() {
    for kind in ["retry", "return_for_changes"] {
        let f = InboxFixture::new();
        let mut wire = f.wire(kind);
        if kind == "return_for_changes" {
            wire["target"]["fingerprint"] = json!(f.delivery());
            wire["text"] = json!("  请补充测试\n保留原文  ");
        }
        let capture = f.capture_once(if kind == "retry" { "failed" } else { "done" }, "turn");
        let submitted = f.call(wire.clone()).expect("新操作回执");
        capture.join().expect("捕获结束");
        assert_eq!(
            submitted["workspace_activity"]["receipt"]["state"], "accepted",
            "Provider未确认时只能返回accepted"
        );
        assert_eq!(
            f.call(wire.clone()).expect("执行中重放"),
            submitted,
            "执行中重放保留accepted回执且不再次派发"
        );
        let call = f.commands.recv_blocking().expect("唯一命令");
        assert!(call.lease.begin(), "预留命令可且仅可执行一次");
        match &call.request.command {
            super::super::MobileCommand::RetryFailed { expected } => {
                assert_eq!(expected.state, "failed", "重试绑定失败状态");
                assert_eq!(expected.turn_id.as_deref(), Some("turn"), "绑定原turn");
            }
            super::super::MobileCommand::FeedbackDelivery { expected, feedback } => {
                assert_eq!(
                    expected.session_id.as_deref(),
                    Some("session"),
                    "反馈只给原会话"
                );
                assert_eq!(
                    feedback.text, "  请补充测试\n保留原文  ",
                    "不能改写用户意见"
                );
                storage::MobileCommandStore::new(f.store.path().to_owned())
                    .prepare_delivery_feedback(&call.source, feedback)
                    .expect("复用反馈持久化");
            }
            _ => panic!("不能派发其他命令"),
        }
        call.reply
            .send(Ok(
                json!({"node_id":1,"run_id":"run","session_id":"session","turn_id":"new-turn"}),
            ))
            .expect("真实新turn确认");
        let store = storage::MobileCommandStore::new(f.store.path().to_owned());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let completed = loop {
            let saved = store
                .get(
                    "phone",
                    &format!("op-{kind}"),
                    &scope(&f.space, &f.area),
                    &f.server.shared.owner_id,
                )
                .expect("查回执")
                .expect("已预留");
            if saved.state != "accepted" {
                break saved;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "回执线程应在5秒内完成"
            );
            std::thread::yield_now();
        };
        assert_eq!(completed.state, "succeeded", "Provider确认后完成操作");
        let first = f.call(wire.clone()).expect("原卡消失后重放");
        assert_eq!(f.call(wire).expect("再次重放"), first, "返回不可变回执");
        assert_eq!(
            first["workspace_activity"]["receipt"]["result"]["turn_id"], "new-turn",
            "回执必须是新turn"
        );
        assert!(
            f.captures.is_empty() && f.commands.is_empty(),
            "重放不能重新捕获或派发"
        );
    }
}

/// 场景：非失败放弃、空反馈和过期目标不得执行或写任何忽略记录。
/// 数据构造：审批/review目标；空白意见；failed卡实际turn为new-turn；错成果指纹。
/// 执行过程：拒绝非法shape→拒绝旧failed turn→拒绝过期fingerprint→查存储及命令队列。
/// 预期结果：非法请求invalid_request，旧目标target_missing/delivery_changed；零忽略与输入。
#[test]
fn inbox_actions_reject_wrong_kind_empty_feedback_and_stale_targets() {
    let f = InboxFixture::new();
    for field in ["request_id", "fingerprint"] {
        let mut wire = f.wire("dismiss");
        wire["target"][field] = json!(if field == "fingerprint" {
            "a".repeat(64)
        } else {
            "request".into()
        });
        assert_eq!(f.call(wire), Err("invalid_request"), "非failed目标不能放弃");
    }
    let mut feedback = f.wire("return_for_changes");
    feedback["target"]["fingerprint"] = json!(f.delivery());
    feedback["text"] = json!(" \n\t");
    assert_eq!(
        f.call(feedback.clone()),
        Err("invalid_request"),
        "空意见不能派发"
    );
    for kind in ["retry", "dismiss"] {
        let capture = f.capture_once("failed", "new-turn");
        assert_eq!(
            f.call(f.wire(kind)),
            Err("target_missing"),
            "旧failed turn不能控制新任务"
        );
        capture.join().expect("捕获结束");
    }
    feedback["text"] = json!("请修改");
    feedback["target"]["fingerprint"] = json!("b".repeat(64));
    let capture = f.capture_once("done", "turn");
    assert_eq!(
        f.call(feedback),
        Err("delivery_changed"),
        "旧成果指纹必须拒绝"
    );
    capture.join().expect("捕获结束");
    assert!(
        storage::MobileCommandStore::new(f.store.path().to_owned())
            .inbox_dismissed_keys(&f.project)
            .expect("忽略键")
            .is_empty(),
        "非法操作不能写忽略记录"
    );
    assert!(f.commands.is_empty(), "拒绝不能发送Provider输入");
}

/// 场景：只读手机不能用任何新操作修改项目或收件箱。
/// 数据构造：隔离项目，原Control授权替换为Read；三个合法操作target。
/// 执行过程：逐个发新操作→检查拒绝→检查捕获/命令/忽略存储。
/// 预期结果：全部unauthorized，零捕获、输入与忽略记录。
#[test]
fn inbox_actions_require_actual_control_grant() {
    let f = InboxFixture::new();
    let grant = f
        .store
        .mobile_access_store()
        .authenticate("phone", &"a".repeat(64))
        .expect("认证")
        .expect("手机");
    f.store
        .mobile_access_store()
        .replace_with_all_projects("phone", grant.revision, storage::MobilePermission::Read)
        .expect("只读授权");
    for kind in ["retry", "dismiss", "return_for_changes"] {
        let mut wire = f.wire(kind);
        if kind == "return_for_changes" {
            wire["target"]["fingerprint"] = json!("a".repeat(64));
            wire["text"] = json!("修改意见");
        }
        assert_eq!(
            f.call(wire),
            Err("unauthorized"),
            "实际Read授权必须拒绝控制操作"
        );
    }
    assert!(
        f.captures.is_empty() && f.commands.is_empty(),
        "拒绝应发生在任何捕获或派发之前"
    );
    assert!(
        storage::MobileCommandStore::new(f.store.path().to_owned())
            .inbox_dismissed_keys(&f.project)
            .expect("键集合")
            .is_empty(),
        "Read不能写忽略记录"
    );
}
