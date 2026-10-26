use super::*;

fn scope(area: &str) -> ContextScope {
    ContextScope {
        workspace_id: "workspace".into(),
        area_id: area.into(),
        project: ContextProject::Project {
            project_id: "project".into(),
            worktree: "/same/repo".into(),
        },
        task_id: None,
        conversation_id: None,
    }
}

fn save_rule(access: &storage::ContextAccess, key: &str, text: &str) {
    let input = access
        .record_input(key, ContextOrigin::Voice, text)
        .unwrap();
    access
        .update(
            key,
            &input.reference,
            &ContextStateUpdate {
                input: input.reference.clone(),
                expected_revision: input.reference.revision,
                change: ContextStateChange::Constraint {
                    key: key.into(),
                    text: text.into(),
                    supersedes: None,
                    enforcement: ConstraintEnforcement::Communicated,
                },
            },
        )
        .unwrap();
}

/// 场景：Live 新通话需接续相同画布的有效约束及报告索引，不能拼入所有原话。
/// 构造：同目录两画布、各自约束、一个带来源的报告和一条无关长输入。
/// 执行：经过生产启动资料准备，再生成实际 session.start JSON。
/// 预期：只包含本画布固定状态与精确索引；不含报告正文、外画布信息或重复用户输入。
#[tokio::test]
async fn gpt_live_start_uses_scoped_state_without_replaying_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let access =
        storage::ContextAccess::new(storage::ContextStore::new(path.clone()), scope("a")).unwrap();
    let other =
        storage::ContextAccess::new(storage::ContextStore::new(path.clone()), scope("b")).unwrap();
    save_rule(&access, "current", "本次先保留接口");
    save_rule(&other, "foreign", "外画布机密要求");
    access
        .record_input("old", ContextOrigin::Voice, "无关历史不要重复提交")
        .unwrap();
    let report = access
        .report("report", "需要按引用读取的报告正文", vec![])
        .unwrap();
    let service = SharedContext::new(path, scope("a")).unwrap();
    let before = service.snapshot().await.unwrap();
    let config = prepare(
        live_voice::LiveConfig {
            instructions: "固定规则".into(),
            voice: "cedar".into(),
        },
        &service,
        "call-a",
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let message = live_voice::LiveSession::new(config)
        .start_message()
        .unwrap();
    let wire: serde_json::Value = serde_json::from_str(&message).unwrap();
    let instructions = wire["session"]["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("本次先保留接口"),
        "有效约束必须进入通话启动消息"
    );
    assert!(instructions.starts_with("固定规则"), "保留现有稳定系统前缀");
    let data: serde_json::Value = serde_json::from_str(instructions.lines().last().unwrap())
        .expect("资料应在独立 JSON 数据块中");
    assert_eq!(data["kind"], "shared_context_data", "必须显式标记为资料");
    assert_eq!(
        data["bundle"]["consumer"], "live_conversation",
        "通话与后台规划消费者分开"
    );
    assert_eq!(
        data["bundle"]["input"]["kind"], "native_audio",
        "不伪造文本输入"
    );
    assert_eq!(
        data["bundle"]["input"]["session_id"], "call-a",
        "绑定当前通话身份"
    );
    assert_eq!(
        data["bundle"]["working_state"][0]["recent_reports"][0]["reference"],
        serde_json::to_value(report.reference).unwrap(),
        "保留可精确补读的来源"
    );
    for excluded in [
        "外画布机密要求",
        "无关历史不要重复提交",
        "需要按引用读取的报告正文",
    ] {
        assert!(
            !instructions.contains(excluded),
            "固定组不能夹带 {excluded}"
        );
    }
    assert_eq!(
        wire["session"]["audio"]["output"]["voice"], "cedar",
        "保留已选音色"
    );
    assert_eq!(
        service.snapshot().await.unwrap(),
        before,
        "读取启动资料不新增用户输入或重放报告"
    );
}

/// 场景：切换或结束已取消的启动不能继续把旧范围资料交给新通话。
/// 构造：独立共享服务与已经取消的通话令牌。
/// 执行：通过相同启动资料准备入口。
/// 预期：明确取消，不返回可连接的配置。
#[tokio::test]
async fn gpt_live_context_cancelled_start_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(dir.path().join("context.sqlite"), scope("a")).unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let result = prepare(
        live_voice::LiveConfig::default(),
        &service,
        "cancelled",
        &token,
    )
    .await;
    assert!(result.is_err(), "取消后不能返回启动配置");
}

/// 场景：固定资料超限或 JSON 转义放大启动消息时，不能裁掉规则继续连接。
/// 构造：两个各 8 KiB 的合法规则；另用 6 KiB 控制字符制造大于 32 KiB 的线包。
/// 执行：调用生产准备入口，再以正常小配置作正对照。
/// 预期：两种超限都明确失败；正常配置可生成小于上限的完整请求。
#[tokio::test]
async fn gpt_live_context_budgets_preserve_required_state_and_count_wire_escaping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let access =
        storage::ContextAccess::new(storage::ContextStore::new(path.clone()), scope("large"))
            .unwrap();
    for key in ["first", "second"] {
        save_rule(&access, key, &"x".repeat(8192));
    }
    let service = SharedContext::new(path.clone(), scope("large")).unwrap();
    let result = prepare(
        live_voice::LiveConfig::default(),
        &service,
        "large",
        &CancellationToken::new(),
    )
    .await;
    assert!(
        result.err().is_some_and(|e| e.contains("required context")),
        "16 KiB 固定状态超限不能静默裁剪"
    );
    let empty = SharedContext::new(path, scope("small")).unwrap();
    let result = prepare(
        live_voice::LiveConfig {
            instructions: "\u{0001}".repeat(6000),
            ..Default::default()
        },
        &empty,
        "escaped",
        &CancellationToken::new(),
    )
    .await;
    assert!(
        result.err().is_some_and(|e| e.contains("32768")),
        "六字节转义后的实际请求必须触发 32 KiB 上限"
    );
    let config = prepare(
        live_voice::LiveConfig::default(),
        &empty,
        "normal",
        &CancellationToken::new(),
    )
    .await
    .expect("正常配置可用");
    assert!(
        live_voice::LiveSession::new(config)
            .start_message()
            .unwrap()
            .len()
            <= 32768,
        "正对照仍在完整请求预算内"
    );
}

/// 场景：后台串行写入占用共享服务时，读取必须可取消且有独立等待上限。
/// 构造：以生产 run 入口持有串行队列；分别在准备已开始后取消，或等待两秒超时。
/// 执行：观察准备先 pending，再等待明确错误，最后释放队列并回收后台工作。
/// 预期：不会返回旧配置；测试另设五秒上限，生产超时失效也不会无限挂住。
#[tokio::test]
async fn gpt_live_context_read_is_cancellable_and_bounded() {
    for cancel in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let service = SharedContext::new(dir.path().join("context.sqlite"), scope("a")).unwrap();
        service.snapshot().await.unwrap();
        let (release, held) = std::sync::mpsc::channel();
        let held = std::sync::Mutex::new(held);
        let (entered, ready) = tokio::sync::oneshot::channel();
        let entered = std::sync::Mutex::new(Some(entered));
        let owner = service.clone();
        let blocker = tokio::spawn(async move {
            owner
                .run(None, move |_| {
                    entered.lock().unwrap().take().unwrap().send(()).unwrap();
                    let _ = held.lock().unwrap().recv();
                    Ok(())
                })
                .await
        });
        ready.await.unwrap();
        let token = CancellationToken::new();
        let result = {
            let work = prepare(
                live_voice::LiveConfig::default(),
                &service,
                "waiting",
                &token,
            );
            let mut work = Box::pin(work);
            assert!(
                futures::poll!(work.as_mut()).is_pending(),
                "先进入实际等待，不能只测试预先取消"
            );
            if cancel {
                token.cancel();
            }
            tokio::time::timeout(std::time::Duration::from_secs(5), work)
                .await
                .expect("独立测试截止时间内应结束")
        };
        release.send(()).unwrap();
        blocker.await.unwrap().unwrap();
        let error = result.err().expect("读取未确认时不得连接");
        assert!(
            error.contains(if cancel { "已取消" } else { "读取超时" }),
            "错误须区分取消和超时：{error}"
        );
    }
}
