use norma::protocol::{BrainOp, ContextProject, ContextRef, ContextScope};

fn scope(area: &str) -> ContextScope {
    ContextScope {
        workspace_id: "workspace-1".into(),
        area_id: area.into(),
        project: ContextProject::Project {
            project_id: "project-1".into(),
            worktree: "/same/worktree".into(),
        },
        task_id: None,
        conversation_id: None,
    }
}

/// 场景：旧版汇报没有来源，新版引用不能跨画布读取。
/// 构造：旧 Report JSON，以及同 workspace / worktree 下两个 area 的引用。
/// 执行：反序列化旧消息，核验引用读取范围，再序列化新版消息。
/// 预期：旧消息来源为空而非猜测；相同目录不能让另一画布获得读取权限。
#[test]
fn legacy_reports_remain_unlinked_and_references_are_area_scoped() {
    let legacy: BrainOp =
        serde_json::from_str(r#"{"Report":{"text":"已拦下","priority":"Normal"}}"#)
            .expect("旧版汇报应保持可读");
    let BrainOp::Report { sources, .. } = legacy else {
        panic!("应为汇报")
    };
    assert!(sources.is_empty(), "旧记录不应虚构决定关联");
    let reference = ContextRef {
        scope: scope("area-1"),
        event_id: "decision-1".into(),
        revision: 7,
    };
    assert!(
        reference.scope.readable_from(&scope("area-1")),
        "本画布可读"
    );
    assert!(
        !reference.scope.readable_from(&scope("area-2")),
        "同目录另一画布不可读"
    );
    let serialized = serde_json::to_string(&reference).expect("引用应可编码");
    assert_eq!(
        serde_json::from_str::<ContextRef>(&serialized).unwrap(),
        reference,
        "引用不能丢失版本或作用域"
    );
}

/// 场景：幕僚可携带来源汇报并通过只读命令补读，不能发送损坏引用。
/// 构造：一个有作用域和版本的 JSON 引用，以及非法 JSON 参数。
/// 执行：解析 report --context-ref 和 context --ref。
/// 预期：两条命令保留相同引用；非法参数在 CLI 边界失败。
#[test]
fn cli_preserves_report_sources_and_read_reference() {
    use norma::{cli::parse_invocation_from, protocol::CardCommand};
    let reference = ContextRef {
        scope: scope("area-1"),
        event_id: "decision-1".into(),
        revision: 7,
    };
    let value = serde_json::to_string(&reference).unwrap();
    let read =
        parse_invocation_from(["norma", "brain", "--token", "t", "context", "--ref", &value])
            .unwrap();
    assert_eq!(
        read.command,
        CardCommand::Brain {
            token: "t".into(),
            op: BrainOp::Context {
                reference: reference.clone()
            }
        },
        "补读应保留完整引用"
    );
    let report = parse_invocation_from([
        "norma",
        "brain",
        "--token",
        "t",
        "report",
        "--text",
        "原因",
        "--context-ref",
        &value,
    ])
    .unwrap();
    let CardCommand::Brain {
        op: BrainOp::Report { sources, .. },
        ..
    } = report.command
    else {
        panic!("应解析为汇报")
    };
    assert_eq!(sources, vec![reference], "汇报来源不能丢失");
    assert!(
        parse_invocation_from(["norma", "brain", "context", "--ref", "bad-json"]).is_err(),
        "非法引用必须拒绝"
    );
}
