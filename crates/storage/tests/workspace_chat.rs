use storage::WorkspaceChatStore;

/// 场景：手机使用过期目录版本修改名称时，不能覆盖 Mac 已修改的配置。
/// 数据构造：空间名称 A；手机缓存版本；Mac 将名称改成 B。
/// 执行过程：读取版本 → 桌面 rename → 手机 rename_if_revision → 读取最终名称。
/// 预期结果：手机被拒绝；名称 B 存在、C 不存在。
#[test]
fn stale_configuration_cannot_overwrite_desktop_change() {
    let dir = tempfile::tempdir().unwrap();
    let store = storage::DesktopWorkspaceStore::new(dir.path().join("app.sqlite"));
    let space = store.create("A").unwrap();
    let revision = store.configuration_revision(&space.id).unwrap();
    store.rename(&space.id, "B").unwrap();
    assert!(
        store.rename_if_revision(&space.id, revision, "C").is_err(),
        "stale phone configuration must conflict"
    );
    assert_eq!(
        store.get(&space.id).unwrap().unwrap().name,
        "B",
        "desktop edit must survive conflict"
    );
    assert_ne!(
        store.get(&space.id).unwrap().unwrap().name,
        "C",
        "rejected mobile name must not be written"
    );
}

/// 场景：跨连接重复提交同一操作只接收一次，重启后回执仍可查询。
///
/// 数据构造：一台 Mac、两个桌面空间、同一设备和操作 ID；正文为单条讨论消息。
/// 执行过程：首次接收 → 同内容重试 → 重开数据库 → 查询回执和事件 → 修改空间重试。
/// 预期结果：相同操作返回同一回执，仅一个事件；另一空间查不到；篡改目标被拒绝。
#[test]
fn receipt_survives_restart_and_transport_retry() {
    let dir = tempfile::tempdir().expect("create isolated test directory");
    let path = dir.path().join("app.sqlite");
    let store = WorkspaceChatStore::new(path.clone());
    let first = store
        .receive("phone", "mac", "space-a", "op", r#"{"text":"讨论首屏"}"#)
        .expect("first receipt");
    let retry = store
        .receive("phone", "mac", "space-a", "op", r#"{"text":"讨论首屏"}"#)
        .expect("retry receipt");
    assert_eq!(first, retry, "retry must return durable original receipt");
    let reopened = WorkspaceChatStore::new(path);
    assert!(
        reopened
            .receipt("phone", "mac", "space-a", "op")
            .unwrap()
            .is_some(),
        "original workspace must retain receipt after restart"
    );
    assert!(
        reopened
            .receipt("phone", "mac", "space-b", "op")
            .unwrap()
            .is_none(),
        "different workspace must not see this receipt"
    );
    assert_eq!(
        reopened.events("mac", "space-a", 0).unwrap().len(),
        1,
        "retry must not append duplicate event"
    );
    assert!(
        reopened
            .receive("phone", "mac", "space-b", "op", r#"{"text":"讨论首屏"}"#)
            .is_err(),
        "same operation cannot be redirected to another workspace"
    );
    assert!(
        reopened
            .receive("phone", "mac", "space-a", "op", r#"{"text":"执行"}"#)
            .is_err(),
        "same operation cannot change its instruction"
    );
}

/// 场景：执行权只可领取一次，回复事件与来源操作长期关联。
///
/// 数据构造：一个讨论操作、一个 JSON 回复；游标从零开始，接收与回复共 2 个事件。
/// 执行过程：接收 → 领取两次 → 保存完成回复 → 从接收游标续读。
/// 预期结果：第二次领取失败；续读仅有回复；旧接收事件不重复。
#[test]
fn claim_once_and_resume_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = WorkspaceChatStore::new(dir.path().join("app.sqlite"));
    let receipt = store
        .receive("phone", "mac", "space", "op", r#"{"text":"hi"}"#)
        .unwrap();
    assert!(
        store.claim("phone", "op").unwrap(),
        "first processor must claim operation"
    );
    assert!(
        !store.claim("phone", "op").unwrap(),
        "second processor must not execute operation twice"
    );
    store
        .finish(
            "phone",
            "op",
            "succeeded",
            r#"{"text":"你好","sender":"Norma"}"#,
        )
        .unwrap();
    let events = store.events("mac", "space", receipt.cursor).unwrap();
    assert_eq!(
        events.len(),
        1,
        "resume should contain only the new response"
    );
    assert_eq!(
        events[0].kind, "succeeded",
        "response must retain actual final state"
    );
    assert_eq!(
        events[0].operation_id, "op",
        "response must retain source operation"
    );
    assert_ne!(
        events[0].cursor, receipt.cursor,
        "resume must not return old receive event"
    );
}

/// 场景：Mac 重启保留已派发任务引用，后续冻结成果仍可补齐到同一来源消息。
/// 数据构造：一条消息、一条含 task_ref 的提交回执、一个迟到的成果。
/// 执行过程：接收并领取 → 保存提交 → 重启恢复 → 写入成果 → 按空间续读。
/// 预期结果：状态为不明确但引用不丢失；成果存在；其它空间没有事件。
#[test]
fn restart_preserves_execution_reference_and_late_result() {
    let dir = tempfile::tempdir().unwrap();
    let store = WorkspaceChatStore::new(dir.path().join("app.sqlite"));
    store
        .receive("phone", "mac", "space", "op", r#"{"text":"run"}"#)
        .unwrap();
    store.claim("phone", "op").unwrap();
    store
        .record_event("phone", "op", "submitted", r#"{"task_ref":"task-1"}"#)
        .unwrap();
    store.reconcile_interrupted().unwrap();
    let receipt = store
        .receipt("phone", "mac", "space", "op")
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.state, "outcome_unknown",
        "restart must not report an unverified success"
    );
    assert_eq!(
        receipt.result.unwrap()["task_ref"],
        "task-1",
        "restart must preserve the real execution reference"
    );
    store
        .record_event(
            "phone",
            "op",
            "task_result",
            r#"{"task_ref":"task-1","text":"result"}"#,
        )
        .unwrap();
    let events = store.events("mac", "space", receipt.cursor).unwrap();
    assert_eq!(
        events.len(),
        1,
        "late frozen result must be appended after restart"
    );
    assert_eq!(
        events[0].mac_id, "mac",
        "event must declare its machine scope"
    );
    assert_eq!(
        events[0].desktop_workspace_id, "space",
        "event must declare its workspace scope"
    );
    assert!(
        store.events("mac", "other", 0).unwrap().is_empty(),
        "other workspace must never see these events"
    );
}

/// 场景：离线期间积累的大消息按字节分页，单页不超过中继帧预算。
/// 数据构造：3 条 64 KiB 正文；页预算 160 KiB，小于中继 256 KiB 上限。
/// 执行过程：保存三条 → 读取首分页 → 从最后游标续读。
/// 预期结果：首分页最多两条；后页包含第三条且没有重复第一条。
#[test]
fn event_pages_respect_relay_byte_budget() {
    let dir = tempfile::tempdir().unwrap();
    let store = WorkspaceChatStore::new(dir.path().join("app.sqlite"));
    let payload = serde_json::json!({"text":"a".repeat(64 * 1024)}).to_string();
    store
        .receive("phone", "mac", "space", "op-1", &payload)
        .unwrap();
    store
        .receive("phone", "mac", "space", "op-2", &payload)
        .unwrap();
    store
        .receive("phone", "mac", "space", "op-3", &payload)
        .unwrap();
    let page = store.events("mac", "space", 0).unwrap();
    assert_eq!(
        page.len(),
        2,
        "160 KiB page must not include three 64 KiB messages"
    );
    assert!(
        serde_json::to_vec(&page).unwrap().len() <= 160 * 1024,
        "serialized page must fit relay frame budget"
    );
    let next = store
        .events("mac", "space", page.last().unwrap().cursor)
        .unwrap();
    assert_eq!(
        next[0].operation_id, "op-3",
        "next page must preserve the third message"
    );
    assert_ne!(
        next[0].operation_id, "op-1",
        "resume must not duplicate first message"
    );
}

/// 场景：桌面增删卡片后，旧手机成员配置版本必须冲突。
/// 数据构造：空间 A 初始版本，成员指纹 empty → card7；相同指纹再次读取。
/// 执行过程：记录初始成员 → 桌面新卡片 → 读相同目录 → 使用旧版本修改。
/// 预期结果：新版本只增加一次；旧版本拒绝且名称未改变。
#[test]
fn desktop_agent_changes_invalidate_mobile_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let store = storage::DesktopWorkspaceStore::new(dir.path().join("app.sqlite"));
    let space = store.create("A").unwrap();
    let before = store
        .observe_agent_configuration(&space.id, "empty")
        .unwrap();
    let after = store
        .observe_agent_configuration(&space.id, "card7")
        .unwrap();
    assert_eq!(
        after,
        before + 1,
        "a desktop card edit must advance configuration revision once"
    );
    assert_eq!(
        store
            .observe_agent_configuration(&space.id, "card7")
            .unwrap(),
        after,
        "unchanged catalog must not create conflict"
    );
    assert!(
        store.rename_if_revision(&space.id, before, "B").is_err(),
        "stale member configuration must not overwrite desktop change"
    );
    assert_eq!(
        store.get(&space.id).unwrap().unwrap().name,
        "A",
        "failed configuration must preserve original name"
    );
}

/// 场景：Mac 重启及发送结果不明确后，任务仍保留真实运行身份与来源。
/// 数据构造：task-1 先提交、随后绑定 run-r/session-s，最后发送结果不明确。
/// 执行过程：接收并领取 → submitted → running → finish unknown → 重开账本查询。
/// 预期结果：恢复查询和回执都保存原运行；不可产生第二个执行引用。
#[test]
fn uncertain_finish_retains_bound_execution() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = WorkspaceChatStore::new(path.clone());
    store
        .receive("phone", "mac", "space", "op", r#"{"text":"run"}"#)
        .unwrap();
    store.claim("phone", "op").unwrap();
    store
        .record_event(
            "phone",
            "op",
            "submitted",
            r#"{"task_ref":"task-1","target":{"node_id":7}}"#,
        )
        .unwrap();
    store.record_event("phone", "op", "running", r#"{"task_ref":"task-1","target":{"node_id":7,"run_id":"run-r","session_id":"session-s"}}"#).unwrap();
    store
        .finish(
            "phone",
            "op",
            "outcome_unknown",
            r#"{"error":"connection_lost"}"#,
        )
        .unwrap();
    let reopened = WorkspaceChatStore::new(path);
    let receipt = reopened
        .receipt("phone", "mac", "space", "op")
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.result.as_ref().unwrap()["task_ref"],
        "task-1",
        "uncertainty must not erase submitted task reference"
    );
    assert_eq!(
        receipt.result.unwrap()["target"]["session_id"],
        "session-s",
        "receipt must retain original provider session"
    );
    let pending = reopened.pending_results("mac", "space").unwrap();
    assert_eq!(
        pending.len(),
        1,
        "binding a runtime must not create another execution"
    );
    assert_eq!(
        pending[0].payload["target"]["run_id"], "run-r",
        "recovery must use frozen original run"
    );
    assert_ne!(
        pending[0].payload["target"]["session_id"],
        serde_json::Value::Null,
        "recovery must not lose session identity"
    );
}

/// 场景：群聊上下文来自持久账本，跨卡片回复与重启后讨论可共享，其它空间不可见。
/// 数据构造：旧讨论、Agent 成果、另一空间的私有消息、当前尚未处理的消息。
/// 执行过程：写入旧事件 → 重开数据库 → 为当前操作读取最近事实。
/// 预期结果：原消息和具名成果存在；当前消息及其它空间消息不重复注入；JSON 不超过 24 KiB。
#[test]
fn shared_context_is_scoped_durable_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.sqlite");
    let store = WorkspaceChatStore::new(path.clone());
    store
        .receive("phone", "mac", "space", "old", r#"{"text":"首屏保留蓝色"}"#)
        .unwrap();
    store.claim("phone", "old").unwrap();
    store
        .record_event(
            "phone",
            "old",
            "task_result",
            r#"{"sender":"Agent","text":"已改为蓝色","task_ref":"task-1"}"#,
        )
        .unwrap();
    store
        .receive(
            "phone",
            "mac",
            "other",
            "private",
            r#"{"text":"别的空间秘密"}"#,
        )
        .unwrap();
    store
        .receive(
            "phone",
            "mac",
            "space",
            "current",
            r#"{"text":"当前新消息"}"#,
        )
        .unwrap();
    let reopened = WorkspaceChatStore::new(path);
    let facts = reopened
        .recent_context("phone", "current")
        .unwrap()
        .to_string();
    assert!(
        facts.contains("首屏保留蓝色"),
        "previous user facts must survive restart"
    );
    assert!(
        facts.contains("已改为蓝色") && facts.contains("task-1"),
        "Agent results must have source task references"
    );
    assert!(
        !facts.contains("别的空间秘密"),
        "other workspace history must not leak into context"
    );
    assert!(
        !facts.contains("当前新消息"),
        "current turn must not be duplicated in historical facts"
    );
    // 24 KiB ≈ 6K ASCII tokens; bounded facts leave room for persona, directory and current input.
    assert!(
        facts.len() <= 24 * 1024,
        "shared context must leave space for current input"
    );
}

/// 场景：消息排队后关联项目被移除，不能使用接收时的旧项目区域继续派工。
/// 数据构造：空间 A 关联真实目录 project；保存区域快照；另一个空空间 B。
/// 执行过程：校验原配置 → 移除区域 → 再校验旧快照与错误空间。
/// 预期结果：原绑定有效；移除后的绑定和错误空间均拒绝。
#[test]
fn removed_area_invalidates_queued_scope() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let store = storage::DesktopWorkspaceStore::new(dir.path().join("app.sqlite"));
    let space = store.create("A").unwrap();
    store.add_project(&space.id, &project).unwrap();
    let areas = store.list_projects(&space.id).unwrap();
    assert!(
        store.mobile_scope_current(&space.id, &areas).unwrap(),
        "current exact region must be valid"
    );
    let other = store.create("B").unwrap();
    assert!(
        !store.mobile_scope_current(&other.id, &areas).unwrap(),
        "another workspace cannot borrow a region"
    );
    let revision = store.configuration_revision(&space.id).unwrap();
    store
        .remove_project_if_revision(&space.id, revision, &areas[0].id)
        .unwrap();
    assert!(
        !store.mobile_scope_current(&space.id, &areas).unwrap(),
        "removed region must invalidate queued messages"
    );
}
