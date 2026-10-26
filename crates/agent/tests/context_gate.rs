use agent::context::*;

fn snapshot() -> ContextSnapshot {
    ContextSnapshot {
        scope: ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        revision: 0,
        state: WorkingState::default(),
        events: vec![],
    }
}

/// 场景：初始加载、保存进行中和持久化失败期间不能把缺状态解释成无限制。
/// 构造：一个空 gate、有效快照，以及限制删除卡片7的下一版本。
/// 执行：依次加载、标 pending、完成提交，再注入失败与越界快照。
/// 预期：仅 ready 时允许；规则生效后拒绝目标7；失败和越界都保持关闭。
#[test]
fn gate_blocks_until_durable_current_state_is_ready() {
    let initial = snapshot();
    let mut gate = ContextExecutionGate::new(initial.scope.clone());
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("7")).is_err(),
        "未加载不能执行"
    );
    gate.install(initial.clone()).unwrap();
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("7")).is_ok(),
        "已核验空状态允许继续原权限校验"
    );
    gate.begin_write();
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("7")).is_err(),
        "写入中不能读旧规则执行"
    );
    let mut saved = initial.clone();
    saved.revision = 1;
    saved.state.constraints.insert(
        "keep".into(),
        ContextConstraint {
            reference: ContextRef {
                scope: initial.scope.clone(),
                event_id: "c".into(),
                revision: 1,
            },
            text: "不要删除卡片7".into(),
            enforcement: ConstraintEnforcement::Enforced {
                operation: ContextOperation::DeleteCard,
                target: Some("7".into()),
            },
        },
    );
    gate.install(saved).unwrap();
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("7")).is_err(),
        "已提交规则阻止目标操作"
    );
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("8")).is_ok(),
        "其他目标仍走原授权"
    );
    assert!(gate.install(initial.clone()).is_err(), "旧快照不能回退限制");
    let mut foreign = initial;
    foreign.scope.area_id = "b".into();
    assert!(gate.install(foreign).is_err(), "不能安装另一个画布的快照");
    assert!(
        gate.check(ContextOperation::DeleteCard, Some("8")).is_err(),
        "不可靠状态保持关闭"
    );
}
