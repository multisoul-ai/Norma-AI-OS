use brain::{ApprovalRequest, ApprovalSubject, BrainEventKind, RouterConfig, ScopedRouter};
use norma::protocol::BrainScope;
use std::time::{Duration, Instant};

fn scope(workspace: &str, route: &str) -> BrainScope {
    BrainScope {
        workspace_id: workspace.into(),
        route_id: route.into(),
    }
}

fn approval() -> BrainEventKind {
    BrainEventKind::ApprovalRequested(ApprovalRequest {
        id: "same-id".into(),
        agent: "Iris".into(),
        subject: ApprovalSubject {
            tool_name: "Bash".into(),
            command: Some("cargo test".into()),
            file_path: None,
            project_root: "/project".into(),
        },
    })
}

/// 场景：两个工作空间都有名为Iris的Agent，同时上报不同进度。
/// 构造：甲乙scope拥有不同route，摘要间隔设为1秒，使用同一单调时钟。
/// 执行：推入两条进度，再推进2秒触发摘要；核对实际交给会话的作用域和文本。
/// 预期：两个来源分别保留，不因同名覆盖；每个提示仅含自己的进度与明确路由身份。
#[test]
fn same_named_agents_keep_independent_scoped_progress() {
    let now = Instant::now();
    let mut router = ScopedRouter::new(RouterConfig {
        summary_every: Some(Duration::from_secs(1)),
        ..RouterConfig::default()
    });
    let alpha = scope("workspace-a", "route-a");
    let beta = scope("workspace-b", "route-b");
    for (source, text) in [(&alpha, "alpha compiling"), (&beta, "beta testing")] {
        assert!(
            router
                .push(
                    source.clone(),
                    BrainEventKind::AgentProgress {
                        agent: "Iris".into(),
                        text: text.into()
                    },
                    now
                )
                .is_empty(),
            "进度先进入摘要，不立即唤醒"
        );
    }
    let actions = router.poll(now + Duration::from_secs(2));
    assert_eq!(actions.len(), 2, "同名Agent的两个作用域各生成摘要");
    for (source, own, other) in [
        (&alpha, "alpha compiling", "beta testing"),
        (&beta, "beta testing", "alpha compiling"),
    ] {
        let action = actions
            .iter()
            .find(|action| &action.scope == source)
            .expect("应保留来源");
        assert!(action.action.text.contains(own), "来源的真实进度必须保留");
        assert!(
            !action.action.text.contains(other),
            "不能以另一空间进度替代本条记录"
        );
        assert!(
            action.action.text.contains(&source.workspace_id),
            "模型必须知道目标工作空间"
        );
        assert!(
            action.action.text.contains(&source.route_id),
            "模型必须知道本次route"
        );
    }
}

/// 场景：相同审批ID在两个route出现，用户只处理其中一个；随后旧route释放。
/// 构造：同一工作空间的两次独立route及相同Agent/审批ID。
/// 执行：关闭甲审批、再次加入甲审批、移除甲route；分别查询双方待审批状态。
/// 预期：关闭/释放甲不会消费乙审批，释放甲也不保留其排队摘要。
#[test]
fn closing_or_releasing_one_route_preserves_other_approval() {
    let now = Instant::now();
    let mut router = ScopedRouter::new(RouterConfig::default());
    let alpha = scope("workspace", "route-a");
    let beta = scope("workspace", "route-b");
    for source in [&alpha, &beta] {
        let actions = router.push(source.clone(), approval(), now);
        assert_eq!(actions.len(), 1, "每个来源审批独立入队");
        assert_eq!(&actions[0].scope, source, "任务必须带准确route");
    }
    router.push(
        alpha.clone(),
        BrainEventKind::ApprovalClosed {
            id: "same-id".into(),
        },
        now,
    );
    assert!(!router.is_open(&alpha, "same-id"), "甲审批已关闭");
    assert!(router.is_open(&beta, "same-id"), "乙同ID审批仍等待");
    router.push(alpha.clone(), approval(), now);
    router.remove(&alpha);
    assert!(!router.is_open(&alpha, "same-id"), "释放route不能残留审批");
    assert!(router.is_open(&beta, "same-id"), "释放甲不能清乙");
}
