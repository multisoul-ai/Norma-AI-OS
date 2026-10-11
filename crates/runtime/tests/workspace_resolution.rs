#[path = "support/workspace_directory.rs"]
mod support;
use norma::workspace_dispatch::*;
use runtime::workspace_resolution::{resolve_workspace_targets, revalidate_workspace_binding};
use support::*;

// 场景：英文大小写及中文语音别名指向同一未加载卡；构造完整两区域目录，
// 分别解析名称；预期正文不变、持久身份精确且没有虚构执行指纹。
#[test]
fn names_and_speech_alias_resolve_cold_card() {
    for name in [" ALAN ", "艾伦"] {
        let WorkspaceResolution::Ready { body, targets } =
            resolve_workspace_targets(&directory(), &request(named(name)))
        else {
            panic!("有效称呼必须解析");
        };
        assert_eq!(body, "修复登录", "任务正文必须原样保留");
        assert_eq!(
            targets[0].card,
            card("first", "alpha"),
            "别名只能绑定已登记实例"
        );
        assert_eq!(
            targets[0].execution, None,
            "未加载卡片不能虚构 Runtime 指纹"
        );
    }
}

// 场景：项目仅一有效候选，但原文点名旧称呼或陌生角色；构造项目限制后解析；
// 预期补全而不退回唯一卡，失败结果保留正文。
#[test]
fn retired_and_unknown_names_never_fall_back_to_project() {
    let catalog = directory();
    for (name, reason) in [
        ("Simon", WorkspaceResolutionReason::RetiredName),
        ("Nobody", WorkspaceResolutionReason::UnknownName),
    ] {
        let target = WorkspaceTargetIntent::CallName {
            name: name.into(),
            project: Some(catalog.projects[0].reference.clone()),
        };
        let result = resolve_workspace_targets(&catalog, &request(target));
        assert_eq!(
            result,
            WorkspaceResolution::NeedsTarget {
                body: "修复登录".into(),
                reason,
                candidates: vec![]
            },
            "明确点名不能改投项目唯一卡"
        );
    }
}

// 场景：同一项目跨两张画布，有相同局部 node_id 的后台卡；从一候选增加为二，
// 再停用全部候选；预期分别自动绑定、选择、补全，不能依据当前视图缩小范围。
#[test]
fn project_zero_one_many_counts_all_areas() {
    let mut catalog = directory();
    let target = WorkspaceTargetIntent::Project {
        project: catalog.projects[0].reference.clone(),
    };
    assert!(
        matches!(
            resolve_workspace_targets(&catalog, &request(target.clone())),
            WorkspaceResolution::Ready { .. }
        ),
        "完整唯一候选必须自动绑定"
    );
    let mut second = catalog.roles[0].clone();
    second.card = card("second", "beta");
    second.name = "Bella".into();
    second.speech_alias = None;
    catalog.call_keys.push(WorkspaceDirectoryCallKey {
        key: "bella".into(),
        card: second.card.clone(),
        state: WorkspaceCallKeyStatus::Active,
    });
    catalog.roles.push(second);
    let WorkspaceResolution::NeedsTarget {
        reason, candidates, ..
    } = resolve_workspace_targets(&catalog, &request(target.clone()))
    else {
        panic!("两张卡必须选择");
    };
    assert_eq!(
        reason,
        WorkspaceResolutionReason::MultipleCandidates,
        "后台卡必须计入候选"
    );
    assert_eq!(candidates.len(), 2, "同 node_id 的独立实例不能合并");
    catalog
        .roles
        .iter_mut()
        .for_each(|role| role.enabled = false);
    assert_eq!(
        resolve_workspace_targets(&catalog, &request(target)),
        WorkspaceResolution::NeedsTarget {
            body: "修复登录".into(),
            reason: WorkspaceResolutionReason::NoCandidates,
            candidates: vec![]
        },
        "停用卡占名但不成为可派工候选"
    );
}

// 场景：目录只读失败导致少一个授权区域；即使已见一张卡也不自动唯一选择；
// 构造缺口后解析项目；预期保留正文并提示目录不完整。
#[test]
fn incomplete_directory_cannot_guess_unique_target() {
    let mut catalog = directory();
    catalog.covered_area_ids.pop();
    catalog.missing_area_ids.push("second".into());
    let target = WorkspaceTargetIntent::Project {
        project: catalog.projects[0].reference.clone(),
    };
    assert_eq!(
        resolve_workspace_targets(&catalog, &request(target)),
        WorkspaceResolution::NeedsTarget {
            body: "修复登录".into(),
            reason: WorkspaceResolutionReason::DirectoryIncomplete,
            candidates: vec![]
        },
        "缺一个区域也不能声称唯一"
    );
}

// 场景：两个不同项目显示同名，或角色实际项目与原文项目限制不一致；
// 添加独立空项目后解析；预期项目歧义与归属不匹配各自拒绝。
#[test]
fn ambiguous_projects_and_project_mismatch_reject() {
    let mut catalog = directory();
    let other = WorkspaceProjectRef {
        workspace_id: "space".into(),
        project_id: "other".into(),
    };
    catalog.projects.push(WorkspaceDirectoryProject {
        reference: other.clone(),
        name: "Web".into(),
        area_ids: vec![],
    });
    let by_name = resolve_workspace_targets(
        &catalog,
        &request(WorkspaceTargetIntent::ProjectName { name: "Web".into() }),
    );
    assert!(
        matches!(
            by_name,
            WorkspaceResolution::NeedsTarget {
                reason: WorkspaceResolutionReason::AmbiguousProject,
                ..
            }
        ),
        "项目同名不得挑第一项"
    );
    let mismatch = resolve_workspace_targets(
        &catalog,
        &request(WorkspaceTargetIntent::CallName {
            name: "Alan".into(),
            project: Some(other),
        }),
    );
    assert!(
        matches!(
            mismatch,
            WorkspaceResolution::NeedsTarget {
                reason: WorkspaceResolutionReason::ProjectMismatch,
                ..
            }
        ),
        "明确项目限制不得静默改变"
    );
}

// 场景：已绑定卡改名后身份不变，随后删除重建同节点；解析后改目录并复核；
// 预期改名保留原归属，删除后不重新选择唯一剩余卡。
#[test]
fn frozen_binding_survives_rename_but_not_recreation_or_disable() {
    let mut catalog = directory();
    let WorkspaceResolution::Ready { targets, .. } =
        resolve_workspace_targets(&catalog, &request(named("Alan")))
    else {
        panic!("初始目标必须有效");
    };
    catalog.roles[0].name = "Bella".into();
    catalog.roles[0].speech_alias = None;
    catalog
        .call_keys
        .iter_mut()
        .for_each(|k| k.state = WorkspaceCallKeyStatus::Retired);
    catalog.call_keys.push(WorkspaceDirectoryCallKey {
        key: "bella".into(),
        card: catalog.roles[0].card.clone(),
        state: WorkspaceCallKeyStatus::Active,
    });
    assert!(
        revalidate_workspace_binding(&catalog, &targets[0]).is_ok(),
        "绑定后改名不改变卡片身份"
    );
    catalog.roles[0].enabled = false;
    assert_eq!(
        revalidate_workspace_binding(&catalog, &targets[0]),
        Err(WorkspaceResolutionReason::DisabledTarget),
        "停用应拒绝原绑定"
    );
    catalog.roles[0].enabled = true;
    catalog.roles[0].card.card_instance_id = "new".into();
    assert_eq!(
        revalidate_workspace_binding(&catalog, &targets[0]),
        Err(WorkspaceResolutionReason::TargetMissing),
        "同节点重建不能恢复旧卡绑定"
    );
}

// 场景：普通讨论和只读查询含明确卡片目标；直接调用公开纯解析；
// 预期两种模式均返回无派工，正文原样且零冻结目标。
#[test]
fn discussion_and_query_never_assign() {
    for mode in [
        WorkspaceRoutingMode::Discussion,
        WorkspaceRoutingMode::Query,
    ] {
        let request = WorkspaceResolutionRequest {
            mode,
            body: "让 Alan 汇报进度".into(),
            targets: vec![named("Alan")],
        };
        assert_eq!(
            resolve_workspace_targets(&directory(), &request),
            WorkspaceResolution::NoDispatch {
                mode,
                body: request.body
            },
            "非执行模式必须无派工"
        );
    }
}
