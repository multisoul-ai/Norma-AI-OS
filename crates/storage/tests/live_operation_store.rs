use agent::{AdapterCommand, AdapterResult};
use storage::{
    LiveOperationRequest, LiveOperationReservation, LiveOperationState, LiveOperationStore,
};

/// AC07/AC10：持久回执在重启后重放，accepted仍表示已接收，同一身份冲突不能重新执行。
/// 数据构造：隔离SQLite、一个创建终端操作、node=9/task=3的accepted回执及不同的新意图。
/// 执行过程：reserve→记录真实类型回执→重建Store并换owner重放→冲突参数→新意图。
/// 预期结果：旧操作返回原回执且不再Claimed；冲突拒绝；明确新意图可Claimed，不把accepted改成完成。
#[test]
fn live_receipt_survives_restart_and_conflicts_do_not_reexecute() {
    let temp = tempfile::tempdir().expect("隔离数据库目录");
    let path = temp.path().join("app.sqlite");
    let store = LiveOperationStore::new(path.clone());
    let command = AdapterCommand::CreateTerminal;
    let request = LiveOperationRequest {
        project_id: "/isolated/project",
        intent_id: "utterance-1",
        operation_id: "step-1",
        owner_id: "boot-a",
        command: &command,
    };
    let lease = match store.reserve(&request).expect("预留操作") {
        LiveOperationReservation::Claimed(lease) => lease,
        _ => panic!("首次操作应获得唯一执行权"),
    };
    let outcome = Ok(AdapterResult::accepted("AgentCard accepted")
        .with_node_id(9)
        .with_task_id(3));
    store.record(&lease, &outcome).expect("持久化已接收回执");
    assert!(
        store
            .record(
                &lease,
                &Ok(AdapterResult::succeeded("different effect").with_node_id(20))
            )
            .is_err(),
        "已知回执不能改成另一个副作用或任务终态"
    );
    assert!(
        store
            .record(
                &lease,
                &Err(agent::AdapterError::Failed("late failure".into()))
            )
            .is_err(),
        "迟到错误也不能抹掉已知回执"
    );
    let restarted = LiveOperationStore::new(path);
    let replay = LiveOperationRequest {
        owner_id: "boot-b",
        ..request
    };
    match restarted.reserve(&replay).expect("重启重放") {
        LiveOperationReservation::Existing(receipt) => {
            assert_eq!(
                receipt.state,
                LiveOperationState::Recorded,
                "重启后的已知回执保持可重放"
            );
            assert_eq!(
                receipt.outcome,
                Some(outcome.clone()),
                "返回原真实类型回执和身份"
            );
            assert_eq!(
                receipt.outcome.expect("已记录").expect("成功回执").status,
                "accepted",
                "accepted不能改报任务完成"
            );
        }
        _ => panic!("重启重放不得重新获得执行权"),
    }
    let changed = AdapterCommand::CreateBrowser { url: None };
    assert!(
        restarted
            .reserve(&LiveOperationRequest {
                command: &changed,
                ..replay
            })
            .is_err(),
        "相同操作身份的不同参数应冲突"
    );
    assert!(
        matches!(
            restarted
                .reserve(&LiveOperationRequest {
                    intent_id: "utterance-2",
                    ..replay
                })
                .expect("新意图"),
            LiveOperationReservation::Claimed(_)
        ),
        "明确新意图仍可执行，不能只按命令内容永久去重"
    );
}

/// AC07：副作用之后的通用Failed不能被当成确认未执行，重启重放必须保持unknown。
/// 数据构造：一个真实隔离项目文件写入，随后Adapter返回无法确定最终身份的Failed。
/// 执行过程：获得执行权→写effect.txt→记录Failed→换owner重启重放→核对文件内容和状态。
/// 预期结果：返回OutcomeUnknown而不是可安全重做的失败；没有新的执行权，文件只有一次副作用。
#[test]
fn live_failure_after_side_effect_requires_reconciliation() {
    let temp = tempfile::tempdir().expect("隔离项目目录");
    let path = temp.path().join("app.sqlite");
    let store = LiveOperationStore::new(path.clone());
    let command = AdapterCommand::CreateTerminal;
    let request = LiveOperationRequest {
        project_id: "/isolated/project",
        intent_id: "utterance-1",
        operation_id: "step-1",
        owner_id: "boot-a",
        command: &command,
    };
    let lease = match store.reserve(&request).expect("预留操作") {
        LiveOperationReservation::Claimed(lease) => lease,
        _ => panic!("首次获得执行权"),
    };
    let effect = temp.path().join("effect.txt");
    std::fs::write(&effect, "created-once\n").expect("真实文件副作用");
    store
        .record(
            &lease,
            &Err(agent::AdapterError::Failed(
                "effect accepted but identity unavailable".into(),
            )),
        )
        .expect("记录不确定失败");
    let restarted = LiveOperationStore::new(path);
    match restarted
        .reserve(&LiveOperationRequest {
            owner_id: "boot-b",
            ..request
        })
        .expect("重放")
    {
        LiveOperationReservation::Existing(receipt) => {
            assert_eq!(
                receipt.state,
                LiveOperationState::OutcomeUnknown,
                "Failed可能发生在副作用后，必须先核对"
            );
            assert!(
                receipt.outcome.expect("错误事实保留").is_err(),
                "原失败事实不能伪装成功"
            );
        }
        _ => panic!("unknown不能自动获得第二次执行权"),
    }
    assert_eq!(
        std::fs::read_to_string(effect).expect("项目文件"),
        "created-once\n",
        "隔离项目中只有一次实际副作用"
    );
}

/// AC07：并发重复请求只能获得一次执行权，未记录回执的旧owner操作在重启后不能接管重做。
/// 数据构造：两个独立SQLite连接、同一稳定意图/步骤、Barrier同时开始及真实append文件。
/// 执行过程：两个线程竞争reserve→只有Claimed写项目文件并记录回执→核对一行→模拟另一未结算步骤重启。
/// 预期结果：并发只Claimed一次；旧owner未结算条目为unknown，既不能新Claim也不能声称成功。
#[test]
fn live_concurrent_claim_is_unique_and_unfinished_restart_is_unknown() {
    use std::io::Write;
    use std::sync::{Arc, Barrier};
    let temp = tempfile::tempdir().expect("隔离项目");
    let path = temp.path().join("app.sqlite");
    let store = LiveOperationStore::new(path.clone());
    let effect = temp.path().join("effects.txt");
    let gate = Arc::new(Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let store = store.clone();
            let effect = effect.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                let command = AdapterCommand::CreateTerminal;
                let request = LiveOperationRequest {
                    project_id: "/isolated/project",
                    intent_id: "utterance-1",
                    operation_id: "step-1",
                    owner_id: "boot-a",
                    command: &command,
                };
                gate.wait();
                match store.reserve(&request).expect("并发预留") {
                    LiveOperationReservation::Claimed(lease) => {
                        let mut file = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(effect)
                            .expect("实际项目文件");
                        writeln!(file, "created-once").expect("实际副作用");
                        file.sync_all().expect("副作用落盘");
                        store
                            .record(
                                &lease,
                                &Ok(AdapterResult::succeeded("created").with_node_id(9)),
                            )
                            .expect("记录回执");
                        true
                    }
                    LiveOperationReservation::Existing(_) => false,
                }
            })
        })
        .collect();
    let claimed = workers
        .into_iter()
        .map(|worker| usize::from(worker.join().expect("并发线程")))
        .sum::<usize>();
    assert_eq!(claimed, 1, "同一意图步骤只有一个事务获得执行权");
    assert_eq!(
        std::fs::read_to_string(effect).expect("项目结果"),
        "created-once\n",
        "实际副作用仅一次，不用mock调用计数替代"
    );
    let command = AdapterCommand::CreateTerminal;
    let pending = LiveOperationRequest {
        project_id: "/isolated/project",
        intent_id: "utterance-1",
        operation_id: "step-2",
        owner_id: "boot-a",
        command: &command,
    };
    assert!(
        matches!(
            store.reserve(&pending).expect("未结算预留"),
            LiveOperationReservation::Claimed(_)
        ),
        "新步骤首次可预留"
    );
    let restarted = LiveOperationStore::new(path);
    match restarted
        .reserve(&LiveOperationRequest {
            owner_id: "boot-b",
            ..pending
        })
        .expect("重启读未结算步骤")
    {
        LiveOperationReservation::Existing(receipt) => {
            assert_eq!(
                receipt.state,
                LiveOperationState::OutcomeUnknown,
                "旧owner未结算操作必须先核对"
            );
            assert!(receipt.outcome.is_none(), "不能补造回执或自动重做");
        }
        _ => panic!("重启不能接管未结算副作用的执行权"),
    }
}

/// AC07/AC08：相同操作键和owner存在于两库时，A库lease不能向B库写入回执。
/// 数据构造：两个独立隔离SQLite库，相同项目/意图/步骤/owner/命令，各自首次Claimed。
/// 执行过程：A得到lease→B得到自己的lease→故意B.record(lease_a)→读取B原预留。
/// 预期结果：跨库lease拒绝，B仍Pending且无回执；不能把隔离/另一账户的node身份串入当前库。
#[test]
fn live_lease_cannot_record_into_another_database() {
    let temp = tempfile::tempdir().expect("两个隔离数据库");
    let first = LiveOperationStore::new(temp.path().join("a.sqlite"));
    let second = LiveOperationStore::new(temp.path().join("b.sqlite"));
    let command = AdapterCommand::CreateTerminal;
    let request = LiveOperationRequest {
        project_id: "/isolated/project",
        intent_id: "utterance-1",
        operation_id: "step-1",
        owner_id: "boot-a",
        command: &command,
    };
    let lease = match first.reserve(&request).expect("A预留") {
        LiveOperationReservation::Claimed(lease) => lease,
        _ => panic!("A首次应认领"),
    };
    assert!(
        matches!(
            second.reserve(&request).expect("B预留"),
            LiveOperationReservation::Claimed(_)
        ),
        "B独立预留首次也应认领"
    );
    assert!(
        second
            .record(
                &lease,
                &Ok(AdapterResult::succeeded("A effect").with_node_id(9))
            )
            .is_err(),
        "A的执行权不能将回执写入B"
    );
    match second.reserve(&request).expect("读取B") {
        LiveOperationReservation::Existing(receipt) => {
            assert_eq!(
                receipt.state,
                LiveOperationState::Pending,
                "B的预留状态保持未执行确认"
            );
            assert!(receipt.outcome.is_none(), "B不得含A的回执");
        }
        _ => panic!("B已有预留不能重新认领"),
    }
}

/// AC07/AC10：按项目核对入口必须跨重启保留原命令与真实ID，并隔离其他项目。
/// 构造：项目A未落账预留、accepted任务42，项目B另一预留；分页上限为100条。
/// 执行：reserve与record后重建Store，按A读取最近操作，分别核查unknown和accepted。
/// 预期：只返回A，缺回执维持unknown，原命令和task/node身份不变，读取不授予重做权。
#[test]
fn live_reconciliation_lists_original_operations_for_only_the_requested_project() {
    let temp = tempfile::tempdir().expect("隔离数据库");
    let path = temp.path().join("app.sqlite");
    let store = LiveOperationStore::new(path.clone());
    for (project, intent) in [("/a", "pending"), ("/a", "accepted"), ("/b", "other")] {
        let request = LiveOperationRequest {
            project_id: project,
            intent_id: intent,
            operation_id: "step-1",
            owner_id: "old-process",
            command: &AdapterCommand::CreateTerminal,
        };
        let LiveOperationReservation::Claimed(lease) = store.reserve(&request).expect("预留")
        else {
            panic!("首次应认领")
        };
        if intent == "accepted" {
            store
                .record(
                    &lease,
                    &Ok(AdapterResult::accepted("任务已接收")
                        .with_task_id(42)
                        .with_node_id(9)),
                )
                .expect("真实回执");
        }
    }
    let rows = LiveOperationStore::new(path)
        .recent("/a", 100)
        .expect("重启查询");
    assert_eq!(rows.len(), 2, "只读本项目操作");
    assert!(
        rows.iter().all(|row| row.key.project_id == "/a"),
        "不得泄漏其他项目"
    );
    let pending = rows
        .iter()
        .find(|row| row.key.intent_id == "pending")
        .expect("pending保留");
    assert_eq!(
        pending.state,
        LiveOperationState::OutcomeUnknown,
        "重启后未落账不能假定失败或可重做"
    );
    assert_eq!(
        pending.command,
        serde_json::json!("CreateTerminal"),
        "核对原始命令"
    );
    let accepted = rows
        .iter()
        .find(|row| row.key.intent_id == "accepted")
        .expect("accepted保留");
    let result = accepted.outcome.clone().expect("已落账").expect("accepted");
    assert_eq!(
        (result.task_id, result.node_id),
        (Some(42), Some(9)),
        "核对真实身份"
    );
}
