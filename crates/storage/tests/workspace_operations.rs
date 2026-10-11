#[path = "support/workspace_operations.rs"]
mod support;
use agent::card_instance::CardInstanceStage;
use storage::{WorkspaceAgentRoleStore, WorkspaceOperationStore};
use support::{setup, source};

/// 整组接收后同来源重发只返回原回执，模型调用ID或新的运行代次不能导致重复。
/// 数据构造：同空间两张精确卡片，来源操作固定；第二次请求只改变准备后的 Runtime 指纹。
/// 执行过程：接收整组、修改 volatile identity 再接收，然后以同键不同正文提交。
/// 预期结果：第一次 new=true、第二次 new=false，仍只有两assignment和原指纹；改参明确冲突。
#[test]
fn atomic_acceptance_is_source_idempotent_and_freezes_original_execution() {
    let (_temp, store, mut request) = setup();
    let original = request.assignments[0].assignment.target.clone();
    let first = store.accept(&request).expect("第一次整组应接收");
    assert!(first.newly_accepted, "第一次须获得新接收权");
    assert_eq!(first.receipt.assignments.len(), 2, "两项目应同事务接收");
    request.assignments[0]
        .assignment
        .target
        .execution
        .runtime_instance_id = "runtime-replaced".into();
    let second = store.accept(&request).expect("同来源重复应返回原记录");
    assert!(!second.newly_accepted, "重复不能重新获得投递权");
    assert_eq!(
        second.receipt.assignments[0].assignment.target, original,
        "原运行指纹不得被重发刷新"
    );
    request.assignments[1].assignment.text = "删除登录".into();
    assert!(store.accept(&request).is_err(), "同键不同稳定参数必须冲突");
    assert_eq!(
        store
            .receipt(&source())
            .expect("应查询")
            .expect("应有记录")
            .assignments
            .len(),
        2,
        "冲突不得新增 assignment"
    );
}

/// 角色在预检后停用时应让整批接收失败，事务不能残留第一项。
/// 数据构造：两有效卡片，目录版本2；预检后只停用第二卡片并保持原expected版本。
/// 执行过程：更改登记、提交整批、只读查询同operation。
/// 预期结果：版本/角色失效拒绝，operation和assignment均不存在。
#[test]
fn disabled_target_rejects_entire_batch_without_partial_rows() {
    let (temp, store, request) = setup();
    WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"))
        .set_enabled(&request.assignments[1].assignment.target.card, false)
        .expect("应停用第二角色");
    assert!(
        store.accept(&request).is_err(),
        "预检后的目录变更应拒绝整批"
    );
    assert_eq!(
        store.receipt(&source()).expect("应只读查询"),
        None,
        "失败不能留下来源operation或第一项"
    );
}

/// 崩溃恢复必须区别排队与可能已写入的传输意图，并从不创建可重放队列。
/// 数据构造：原子接收两项，只为第一项commit Armed，第二项仍queued。
/// 执行过程：arm 第一项后调用恢复两次，再读取原回执与事件。
/// 预期结果：第一项Unknown且不安全重试，第二项Interrupted且可安全手动重试；重复恢复零新增事件。
#[test]
fn restart_marks_armed_unknown_and_queued_interrupted_without_replay() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("应接收两项");
    store.arm_input("assignment-a").expect("应先持久化传输意图");
    store.reconcile_interrupted().expect("恢复应成功");
    let receipt = store
        .receipt(&source())
        .expect("应读取")
        .expect("原操作存在");
    assert_eq!(
        receipt.assignments[0].receipt.stage,
        CardInstanceStage::OutcomeUnknown,
        "可能写入者只能待核对"
    );
    assert!(
        !receipt.assignments[0]
            .receipt
            .failure
            .as_ref()
            .expect("未知须有原因")
            .retry_safe,
        "未知不能盲目重试"
    );
    assert_eq!(
        receipt.assignments[1].receipt.stage,
        CardInstanceStage::Interrupted,
        "未armed项应中断"
    );
    assert!(
        receipt.assignments[1]
            .receipt
            .failure
            .as_ref()
            .expect("中断须有原因")
            .retry_safe,
        "零输入项可手动安全重试"
    );
    let cursor = receipt.cursor;
    store.reconcile_interrupted().expect("重复恢复应幂等");
    assert_eq!(
        store
            .receipt(&source())
            .expect("读取重复恢复")
            .expect("记录存在")
            .cursor,
        cursor,
        "恢复不能重复写事件或重放"
    );
}

/// 严格只读查询缺文件时不能创建SQLite、迁移或登记。
/// 数据构造：全新的不存在 AppDB 路径。
/// 执行过程：查询operation和分页事件。
/// 预期结果：结果为空且文件继续不存在。
#[test]
fn readonly_queries_do_not_create_missing_database() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("missing.sqlite");
    let store = WorkspaceOperationStore::new(path.clone());
    assert_eq!(
        store.receipt(&source()).expect("缺库应返回空"),
        None,
        "未知来源不得伪造回执"
    );
    assert!(
        store
            .events("alpha", 0, 100)
            .expect("缺库事件应为空")
            .is_empty(),
        "缺库无事件"
    );
    assert!(!path.exists(), "只读API不能创建缺失数据库");
}

/// 相同来源操作若改变attempt_sequence，应构成改参冲突而不能返回旧回执。
/// 数据构造：成功接收A/B原组，复制同来源并把A顺序由1改为2。
/// 执行过程：重发改参请求，再读取原组。
/// 预期结果：明确拒绝，原顺序1不改变且不授予新的投递权。
#[test]
fn changed_attempt_sequence_conflicts_with_same_source_operation() {
    let (_temp, store, mut request) = setup();
    store.accept(&request).expect("应接收原组");
    request.assignments[0].attempt_sequence = 2;
    assert!(
        store.accept(&request).is_err(),
        "attempt顺序是稳定参数，同操作改动必须冲突"
    );
    assert_eq!(
        store
            .receipt(&source())
            .expect("应读取原操作")
            .expect("原操作存在")
            .assignments[0]
            .attempt_sequence,
        1,
        "冲突不能改动原顺序"
    );
}

/// 已接收来源的旧名称失效后，原参数只读lookup仍返回原卡；换结构化目标必须冲突。
/// 数据构造：A/B原组成功，随后A由Alan改名Aaron；完整原请求指纹为64位hash。
/// 执行过程：旧来源原指纹lookup，改指纹lookup并accept，再读持久原回执。
/// 预期结果：原lookup零准备且冻结身份不变；相同op不同原参数拒绝，不能借旧回执掩盖变更。
#[test]
fn original_request_lookup_precedes_retired_name_resolution_and_rejects_changed_inputs() {
    let (temp, store, mut request) = setup();
    let accepted = store.accept(&request).expect("应接收原组");
    WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"))
        .rename(&request.assignments[0].assignment.target.card, "Aaron")
        .expect("应改名并使Alan失效");
    let repeated = store
        .lookup_original_request(&request.source, &request.original_request_fingerprint)
        .expect("重复原请求应只读命中")
        .expect("原操作存在");
    assert_eq!(
        repeated.assignments[0].assignment.target,
        accepted.receipt.assignments[0].assignment.target,
        "改名不能更新历史身份或要求重新解析Alan"
    );
    request.original_request_fingerprint = "b".repeat(64);
    assert!(
        store
            .lookup_original_request(&request.source, &request.original_request_fingerprint)
            .is_err(),
        "同op改变mode或结构化目标应原参数冲突"
    );
    assert!(
        store.accept(&request).is_err(),
        "直接接收也不能忽略原参数冲突"
    );
    assert_eq!(
        store
            .receipt(&source())
            .expect("读原记录")
            .expect("原操作保留")
            .original_request_fingerprint,
        "d0".repeat(32),
        "冲突不能覆盖原封套指纹"
    );
}

/// 缺库的原请求lookup必须严格只读，不能为了幂等查询建库或迁移。
/// 数据构造：不存在的AppDB与合法source/64字节指纹。
/// 执行过程：只读lookup_original_request，再检查文件。
/// 预期结果：无原回执，SQLite文件仍不存在。
#[test]
fn original_request_lookup_does_not_create_a_missing_database() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let path = temp.path().join("missing-original.sqlite");
    let store = WorkspaceOperationStore::new(path.clone());
    assert!(
        store
            .lookup_original_request(&source(), &"a".repeat(64))
            .expect("缺库返回空")
            .is_none(),
        "不能伪造已接收回执"
    );
    assert!(!path.exists(), "原参数lookup不能建库");
}

/// 两个并发相同来源提交只允许一个拿到新投递权，另一个返回同一持久回执。
/// 数据构造：同一AppDB和同一个原组，两个线程用Barrier同时开始accept。
/// 执行过程：并发接收、合并两个结果，再查询持久操作。
/// 预期结果：newly_accepted之和为1；两回执冻结参数相同，数据库只含原两项。
#[test]
fn concurrent_same_source_has_exactly_one_delivery_winner() {
    let (_temp, store, request) = setup();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let store = store.clone();
            let request = request.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.accept(&request).expect("并发提交应串行返回同一操作")
            })
        })
        .collect();
    let received: Vec<_> = threads
        .into_iter()
        .map(|t| t.join().expect("接收线程不能panic"))
        .collect();
    assert_eq!(
        received.iter().filter(|r| r.newly_accepted).count(),
        1,
        "新投递权只有一个winner"
    );
    assert_eq!(
        received[0].receipt, received[1].receipt,
        "并发重复返回原冻结回执"
    );
    assert_eq!(
        store
            .receipt(&source())
            .expect("读持久原组")
            .expect("存在")
            .assignments
            .len(),
        2,
        "只能有原两项，没有重复队列权"
    );
}

/// 同origin/operation改workspace是同键改参，必须在prepare前的只读lookup拒绝。
/// 数据构造：原操作在alpha接收，复制source只把workspace改beta。
/// 执行过程：原参数lookup与稳定计划lookup均读取改scope来源。
/// 预期结果：两种lookup都报冲突而不是None，原alpha回执不被刷新或暴露为beta记录。
#[test]
fn changed_workspace_conflicts_during_readonly_lookup_before_preparation() {
    let (_temp, store, request) = setup();
    store.accept(&request).expect("接收原alpha组");
    let mut changed = request.source.clone();
    changed.workspace_id = "beta".into();
    assert!(
        store
            .lookup_original_request(&changed, &request.original_request_fingerprint)
            .is_err(),
        "原参数lookup应在换空间时冲突，不能进入prepare"
    );
    let fingerprint = storage::workspace_operation_fingerprint(&changed, &request.assignments)
        .expect("稳定改scopehash");
    assert!(
        store.lookup(&changed, &fingerprint).is_err(),
        "计划lookup也不能把既有来源键当新操作"
    );
    assert!(
        store.receipt(&changed).expect("scope查询应只读").is_none(),
        "beta不能得到alpha原回执"
    );
    assert_eq!(
        store
            .receipt(&source())
            .expect("读原alpha")
            .expect("原操作存在")
            .source
            .workspace_id,
        "alpha",
        "原归属不变"
    );
}
