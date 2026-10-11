#[path = "support/workspace_directory.rs"]
mod support;
use agent::workspace_plan::{
    apply_workspace_target_selections, build_original_target_manifest,
    confirm_workspace_target_selection, validate_workspace_task_plan, WorkspaceCompletionError,
};
use norma::workspace_dispatch::*;
use support::*;

// 场景：用户用失效Simon派工后主动选择有效Alan；从原消息创建可信选择，
// 仅用于同一代码生成操作scope，校验有效manifest；预期正文/原审计保留，旧键仍失效。
#[test]
fn user_selection_can_complete_retired_target_without_changing_original_audit() {
    let directory = directory();
    let body = "让 Simon 修复登录";
    let original =
        build_original_target_manifest(&directory, WorkspaceRoutingMode::Execute, body, &[]);
    let selected = directory.roles[0].card.clone();
    let token = confirm_workspace_target_selection(
        "space:desktop:operation:message",
        &original,
        0,
        &directory,
        &selected,
    )
    .expect("明确用户选择应可补全原旧名称缺口");
    let completed =
        apply_workspace_target_selections("space:desktop:operation:message", &original, &[token])
            .expect("相同来源可应用可信选择");
    assert_eq!(
        completed.original(),
        &original,
        "原消息manifest须完整保留供审计"
    );
    assert_eq!(
        completed.effective().original_body,
        body,
        "补全不能重写任务正文"
    );
    assert_eq!(
        completed.effective().targets,
        vec![WorkspaceTargetIntent::Card {
            card: selected.clone(),
            project: Some(directory.roles[0].project.clone())
        }],
        "只有用户选中的精确卡片可替换旧名称"
    );
    assert!(
        directory
            .call_keys
            .iter()
            .any(|k| k.key == "simon" && k.state == WorkspaceCallKeyStatus::Retired),
        "补全不能复活旧呼叫键"
    );
    let target = FrozenWorkspaceTarget {
        card: selected,
        project: directory.roles[0].project.clone(),
        name_at_binding: "Alan".into(),
        directory_revision: directory.revision,
        execution: None,
    };
    let resolution = WorkspaceResolution::Ready {
        body: body.into(),
        targets: vec![target.clone()],
    };
    assert_eq!(
        validate_workspace_task_plan(completed.effective(), &resolution, &[target]),
        Ok(()),
        "可信用户补全后的完整计划可以继续原任务"
    );
}

// 场景：相同原文的两次发送拥有不同操作，或同一操作正文后来被编辑；
// 构造一份选择token并尝试跨来源应用；预期不能把用户选择搬到另一条任务。
#[test]
fn selection_is_bound_to_exact_operation_and_original_manifest() {
    let directory = directory();
    let original = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Simon 检查登录",
        &[],
    );
    let selection = confirm_workspace_target_selection(
        "space:origin:op1:msg1",
        &original,
        0,
        &directory,
        &directory.roles[0].card,
    )
    .expect("原消息可生成可信选择");
    assert_eq!(
        apply_workspace_target_selections("space:origin:op2:msg2", &original, &[selection]).err(),
        Some(WorkspaceCompletionError::ScopeMismatch),
        "同样正文的另一次发送不能复用选择权限"
    );
    let selection = confirm_workspace_target_selection(
        "space:origin:op1:msg1",
        &original,
        0,
        &directory,
        &directory.roles[0].card,
    )
    .expect("原消息可生成第二份可信选择");
    let changed = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Simon 删除登录",
        &[],
    );
    assert_eq!(
        apply_workspace_target_selections("space:origin:op1:msg1", &changed, &[selection]).err(),
        Some(WorkspaceCompletionError::ScopeMismatch),
        "正文编辑后原选择证据不能沿用"
    );
}

// 场景：两个明确旧/未知收件人，用户只补全第一个；构造同一完整原文manifest，
// 应用单项选择并校验计划；预期第二个缺口保留，整组仍零接收。
#[test]
fn completing_one_recipient_does_not_drop_another_unresolved_recipient() {
    let directory = directory();
    let original = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Simon 和 Nobody 分别检查登录",
        &[],
    );
    assert!(original.complete, "明确并列称呼可以完整提取原文证据");
    let selection = confirm_workspace_target_selection(
        "scope",
        &original,
        0,
        &directory,
        &directory.roles[0].card,
    )
    .expect("旧名称缺口允许用户选卡");
    let completed = apply_workspace_target_selections("scope", &original, &[selection])
        .expect("只替换第一个明确目标");
    assert_eq!(
        completed.effective().targets[1],
        named("Nobody"),
        "第二未知目标仍须独立补全"
    );
    assert_eq!(
        completed.effective().targets.len(),
        2,
        "不能删除第二收件人以凑完整计划"
    );
    let unresolved = WorkspaceResolution::NeedsTarget {
        body: original.original_body.clone(),
        reason: WorkspaceResolutionReason::UnknownName,
        candidates: vec![],
    };
    assert_eq!(
        validate_workspace_task_plan(completed.effective(), &unresolved, &[]),
        Err(WorkspacePlanError::UnresolvedOriginalTargets),
        "未补齐完整目标全集时必须零接收"
    );
}

// 场景：原文非执行或有讨论/否定/语法缺口，仍出现已失效角色；
// 尝试生成用户选择token；预期选卡不能解除执行门控，UI应保留正文并要求新明确来源。
#[test]
fn target_selection_cannot_upgrade_non_execution_or_unproved_source() {
    let directory = directory();
    for (mode, body) in [
        (WorkspaceRoutingMode::Discussion, "让 Simon 检查登录"),
        (WorkspaceRoutingMode::Query, "让 Simon 汇报登录"),
        (WorkspaceRoutingMode::Execute, "讨论一下：让 Simon 检查登录"),
        (WorkspaceRoutingMode::Execute, "@Simon 如何分派任务"),
        (WorkspaceRoutingMode::Execute, "不要让 Simon 删除数据"),
        (WorkspaceRoutingMode::Execute, "让陌生角色修复登录"),
    ] {
        let original = build_original_target_manifest(&directory, mode, body, &[]);
        assert_eq!(
            confirm_workspace_target_selection(
                "scope",
                &original,
                0,
                &directory,
                &directory.roles[0].card
            )
            .err(),
            Some(WorkspaceCompletionError::UnprovedSource),
            "用户选卡不能静默升级原文: {body}"
        );
    }
}

// 场景：原文明确Web项目但用户选中API卡；把第二画布构成独立API项目与有效Bella，
// 尝试补全旧Simon；预期精确身份不丢弃原文项目限制，同Web卡仍可选。
#[test]
fn selection_respects_literal_project_constraint_and_current_card_validity() {
    let mut directory = directory();
    let api = WorkspaceProjectRef {
        workspace_id: "space".into(),
        project_id: "api".into(),
    };
    directory.projects[0].area_ids = vec!["first".into()];
    directory.areas[1].project = api.clone();
    directory.areas[1].name = "API".into();
    directory.projects.push(WorkspaceDirectoryProject {
        reference: api.clone(),
        name: "API".into(),
        area_ids: vec!["second".into()],
    });
    let mut bella = directory.roles[0].clone();
    bella.card = card("second", "beta");
    bella.project = api;
    bella.name = "Bella".into();
    bella.speech_alias = None;
    directory.call_keys.push(WorkspaceDirectoryCallKey {
        key: "bella".into(),
        card: bella.card.clone(),
        state: WorkspaceCallKeyStatus::Active,
    });
    directory.roles.push(bella.clone());
    let original = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Web 项目的 Simon 修复登录",
        &[],
    );
    assert!(
        directory.is_consistent(),
        "必须使用真实完整多项目目录构造反例"
    );
    assert_eq!(
        confirm_workspace_target_selection("scope", &original, 0, &directory, &bella.card).err(),
        Some(WorkspaceCompletionError::ProjectMismatch),
        "用户选卡不能丢掉原文明确项目限制"
    );
    assert!(
        confirm_workspace_target_selection(
            "scope",
            &original,
            0,
            &directory,
            &directory.roles[0].card
        )
        .is_ok(),
        "用户选择原文项目的有效卡可以继续"
    );
    directory.roles[0].enabled = false;
    assert_eq!(
        confirm_workspace_target_selection(
            "scope",
            &original,
            0,
            &directory,
            &directory.roles[0].card
        )
        .err(),
        Some(WorkspaceCompletionError::InvalidSelectedTarget),
        "停用卡不能因用户选中而获得执行权限"
    );
}

// 场景：用户只指定Web项目，该项目两个画布各有有效角色；构造持久目录和
// 原文项目manifest，用户选择第二卡；预期可信选择精确绑定第二卡并保留原项目证据。
#[test]
fn user_can_complete_multiple_candidates_inside_original_project() {
    let mut directory = directory();
    let mut bella = directory.roles[0].clone();
    bella.card = card("second", "beta");
    bella.name = "Bella".into();
    bella.speech_alias = None;
    directory.call_keys.push(WorkspaceDirectoryCallKey {
        key: "bella".into(),
        card: bella.card.clone(),
        state: WorkspaceCallKeyStatus::Active,
    });
    directory.roles.push(bella.clone());
    let original = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Web 项目检查登录",
        &[],
    );
    assert_eq!(
        original.targets,
        vec![WorkspaceTargetIntent::ProjectName { name: "Web".into() }],
        "原文只限定项目而未点名角色"
    );
    let token =
        confirm_workspace_target_selection("project-scope", &original, 0, &directory, &bella.card)
            .expect("用户可在原明确项目的多个候选中选择具体卡");
    let completed = apply_workspace_target_selections("project-scope", &original, &[token])
        .expect("可信项目选择可生成执行视图");
    assert_eq!(
        completed.effective().targets,
        vec![WorkspaceTargetIntent::Card {
            card: bella.card.clone(),
            project: Some(bella.project.clone())
        }],
        "第二画布卡片必须按稳定身份绑定"
    );
    assert_eq!(
        completed.original(),
        &original,
        "原项目字面限制与证据必须保留"
    );
    let mut exact = original.clone();
    exact.targets[0] = WorkspaceTargetIntent::Project {
        project: directory.projects[0].reference.clone(),
    };
    exact.evidence[0].target = exact.targets[0].clone();
    assert!(
        confirm_workspace_target_selection("exact-project", &exact, 0, &directory, &bella.card)
            .is_ok(),
        "可信结构化项目引用也须允许在原项目内选择卡片"
    );
}

// 场景：两个不同项目显示名均Web，用户通过项目上下文选中第二项目具体卡；
// 构造两个精确project_ref和同名字面目标，再尝试转给其他项目；预期选择澄清归属且不跨原项目集合。
#[test]
fn user_card_selection_disambiguates_same_named_projects_without_losing_scope() {
    let mut directory = directory();
    let second = WorkspaceProjectRef {
        workspace_id: "space".into(),
        project_id: "other-project".into(),
    };
    directory.projects[0].area_ids = vec!["first".into()];
    directory.areas[1].project = second.clone();
    directory.areas[1].name = "Web".into();
    directory.projects.push(WorkspaceDirectoryProject {
        reference: second.clone(),
        name: "Web".into(),
        area_ids: vec!["second".into()],
    });
    let mut bella = directory.roles[0].clone();
    bella.card = card("second", "beta");
    bella.project = second.clone();
    bella.name = "Bella".into();
    bella.speech_alias = None;
    directory.call_keys.push(WorkspaceDirectoryCallKey {
        key: "bella".into(),
        card: bella.card.clone(),
        state: WorkspaceCallKeyStatus::Active,
    });
    directory.roles.push(bella.clone());
    assert!(
        directory.is_consistent(),
        "反例必须使用完整合法的同名多项目目录"
    );
    let original = build_original_target_manifest(
        &directory,
        WorkspaceRoutingMode::Execute,
        "让 Web 项目检查登录",
        &[],
    );
    let selection = confirm_workspace_target_selection(
        "ambiguous-project",
        &original,
        0,
        &directory,
        &bella.card,
    )
    .expect("用户选卡应同时澄清同名项目归属");
    let completed = apply_workspace_target_selections("ambiguous-project", &original, &[selection])
        .expect("同名项目选择可以保留原文并精确绑定");
    assert_eq!(
        completed.effective().targets,
        vec![WorkspaceTargetIntent::Card {
            card: bella.card.clone(),
            project: Some(second)
        }],
        "必须绑定用户选择的第二project而非第一同名项目"
    );
    let mut exact = original.clone();
    exact.targets[0] = WorkspaceTargetIntent::Project {
        project: directory.projects[0].reference.clone(),
    };
    exact.evidence[0].target = exact.targets[0].clone();
    assert_eq!(
        confirm_workspace_target_selection(
            "exact-first-project",
            &exact,
            0,
            &directory,
            &bella.card
        )
        .err(),
        Some(WorkspaceCompletionError::ProjectMismatch),
        "精确项目引用不能因显示名相同被改投另一project"
    );
    directory.projects[1].name = "API".into();
    directory.areas[1].name = "API".into();
    assert_eq!(
        confirm_workspace_target_selection("original-Web", &original, 0, &directory, &bella.card)
            .err(),
        Some(WorkspaceCompletionError::ProjectMismatch),
        "不符合原字面项目条件的卡片不能选中"
    );
}
