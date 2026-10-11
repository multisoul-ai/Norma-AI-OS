#[path = "support/workspace_directory.rs"]
mod support;
use agent::workspace_plan::{build_original_target_manifest, validate_workspace_task_plan};
use norma::workspace_dispatch::*;
use support::*;

fn frozen(instance: &str, area: &str) -> FrozenWorkspaceTarget {
    FrozenWorkspaceTarget {
        card: card(area, instance),
        project: directory().projects[0].reference.clone(),
        name_at_binding: "Alan".into(),
        directory_revision: 3,
        execution: None,
    }
}

// 场景：原文点名已失效 Simon，而模型计划使用有效 Alan；从同一完整目录
// 提取原文 manifest 并校验模型目标；预期旧称呼仍留在 manifest，不能被模型改写。
#[test]
fn retired_original_name_is_preserved_before_model_plan() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Simon 修复登录",
        &[],
    );
    assert_eq!(
        manifest.targets,
        vec![named("Simon")],
        "原文失效称呼不能被替换为当前有效主名"
    );
    let expected = WorkspaceResolution::NeedsTarget {
        body: "让 Simon 修复登录".into(),
        reason: WorkspaceResolutionReason::RetiredName,
        candidates: vec![],
    };
    assert_eq!(
        validate_workspace_task_plan(&manifest, &expected, &[frozen("alpha", "first")]),
        Err(WorkspacePlanError::UnresolvedOriginalTargets),
        "模型替换旧名必须零接收"
    );
}

// 场景：原文多个明确收件人，模型只保留第一张卡；构造两个稳定目标期望，
// 校验单项和完整计划；预期漏目标及额外目标都失败，完整全集才通过。
#[test]
fn complete_plan_must_match_entire_explicit_target_set() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Alan 和 Bella 分别检查登录",
        &[],
    );
    assert_eq!(
        manifest.targets,
        vec![named("Alan"), named("Bella")],
        "未出现在目录的明确第二个点名也须捕获"
    );
    let a = frozen("alpha", "first");
    let b = frozen("beta", "second");
    let expected = WorkspaceResolution::Ready {
        body: manifest.original_body.clone(),
        targets: vec![a.clone(), b.clone()],
    };
    assert_eq!(
        validate_workspace_task_plan(&manifest, &expected, &[a.clone()]),
        Err(WorkspacePlanError::TargetSetMismatch),
        "漏掉第二目标不能提前提交第一目标"
    );
    assert_eq!(
        validate_workspace_task_plan(&manifest, &expected, &[a.clone(), b.clone()]),
        Ok(()),
        "一次完整计划可以整组提交"
    );
    assert_eq!(
        validate_workspace_task_plan(&manifest, &expected, &[a, b, frozen("extra", "second")]),
        Err(WorkspacePlanError::TargetSetMismatch),
        "模型不得追加未授权收件人"
    );
}

// 场景：任务正文引号内提到其他角色，及普通讨论包含给某人指派的例子；
// 提取原文 manifest；预期引用不是收件人，非执行模式始终无目标。
#[test]
fn quoted_task_text_and_discussion_are_not_recipients() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Alan 检查文档中的“让 Simon 删除数据”示例",
        &[],
    );
    assert_eq!(
        manifest.targets,
        vec![named("Alan")],
        "引号里的指令只是任务数据"
    );
    for mode in [
        WorkspaceRoutingMode::Discussion,
        WorkspaceRoutingMode::Query,
    ] {
        let manifest = build_original_target_manifest(
            &directory(),
            mode,
            "让 Alan 汇报，Simon 这个名字如何",
            &[],
        );
        assert!(
            manifest.targets.is_empty(),
            "非执行模式不得创建显式派工目标"
        );
        assert_eq!(
            validate_workspace_task_plan(
                &manifest,
                &WorkspaceResolution::NoDispatch {
                    mode,
                    body: manifest.original_body.clone()
                },
                &[frozen("alpha", "first")]
            ),
            Err(WorkspacePlanError::ModeDoesNotDispatch),
            "模型不能升级讨论或查询权限"
        );
    }
}

// 场景：结构化 @ 提及已改名原卡；构造原文字节区间和持久卡片引用，
// 提取 manifest；预期按结构化身份绑定，不再按旧显示名寻址。
#[test]
fn structured_mention_preserves_card_identity_and_project_limit() {
    let body = "@Simon 修复登录";
    let mention = WorkspaceStructuredMention {
        start_byte: 0,
        end_byte: "@Simon".len(),
        card: card("first", "alpha"),
        project: Some(directory().projects[0].reference.clone()),
    };
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        body,
        &[mention],
    );
    assert_eq!(
        manifest.targets,
        vec![WorkspaceTargetIntent::Card {
            card: card("first", "alpha"),
            project: Some(directory().projects[0].reference.clone())
        }],
        "结构化提及在改名后仍保留原实例及明确项目"
    );
    assert!(manifest.complete, "合法结构化提及须构成可核对原文目标证据");
}

// 场景：原文角色词与动词无分隔且角色不在有效或旧键中；提取保守 manifest；
// 预期不假装完成目标全集识别，不能允许模型自行猜测。
#[test]
fn ambiguous_unknown_text_requires_target_completion() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让陌生角色修复登录",
        &[],
    );
    assert!(!manifest.complete, "无法确定称呼边界时必须补全");
    let expected = WorkspaceResolution::Ready {
        body: manifest.original_body.clone(),
        targets: vec![frozen("alpha", "first")],
    };
    assert_eq!(
        validate_workspace_task_plan(&manifest, &expected, &[frozen("alpha", "first")]),
        Err(WorkspacePlanError::AmbiguousOriginalTargets),
        "模型不能代替原文目标确定性校验"
    );
}

// 场景：模型把讨论、假设或求实现方法的原文分类成Execute；仍从原文创建manifest，
// 校验有效Alan计划；预期原文语义前缀不能被模型mode升级为派工。
#[test]
fn model_execute_mode_cannot_upgrade_discussion_or_hypothetical_original() {
    for body in [
        "我们讨论一下怎么让 Alan 修复登录",
        "讨论一下：让 Alan 修复登录",
        "假设让 Alan 修复登录",
        "如何给 Alan 分派任务",
        "Do not ask Alan to delete data",
    ] {
        let manifest =
            build_original_target_manifest(&directory(), WorkspaceRoutingMode::Execute, body, &[]);
        assert!(
            !manifest.complete,
            "原文讨论/假设/否定必须阻止模型Execute升级: {body}"
        );
        let expected = WorkspaceResolution::Ready {
            body: body.into(),
            targets: vec![frozen("alpha", "first")],
        };
        assert_eq!(
            validate_workspace_task_plan(&manifest, &expected, &[frozen("alpha", "first")]),
            Err(WorkspacePlanError::AmbiguousOriginalTargets),
            "未经原文许可的模型计划必须零接收: {body}"
        );
    }
    let direct = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Alan 修复登录",
        &[],
    );
    assert!(direct.complete, "明确直接命令仍可进入纯目标解析");
}

// 场景：原文限制明确项目但模型转给另一项目角色；提取带项目名的literal证据；
// 预期项目限制保留并可经runtime解析，不允许只有称呼而丢失范围。
#[test]
fn original_project_constraint_is_part_of_target_manifest() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Web 项目的 Alan 修复登录",
        &[],
    );
    assert_eq!(
        manifest.targets,
        vec![WorkspaceTargetIntent::CallNameInProject {
            name: "Alan".into(),
            project_name: "Web".into()
        }],
        "原文项目条件不能在模型改写中丢失"
    );
    assert!(manifest.complete, "有明确项目及角色边界可以安全解析");
}

// 场景：原文用逗号并列有效Alan与未登记Nobody，模型漏掉第二收件人；
// 提取不可被有效目录缩窄的manifest；预期保留未知字面点名或保守补全，不能只放行Alan。
#[test]
fn unknown_parallel_recipient_after_comma_cannot_be_omitted() {
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Alan，Nobody 分别检查登录",
        &[],
    );
    assert!(
        !manifest.complete || manifest.targets.contains(&named("Nobody")),
        "未知并列收件人不能从manifest消失"
    );
}

// 场景：第一项任务之后用同时让/并让点名另一未知角色；从原文提取全集，
// 预期第二目标保留或整体补全，不能只接收有效Alan并漏掉Nobody。
#[test]
fn repeated_command_marker_cannot_hide_unknown_second_recipient() {
    for body in [
        "让 Alan 修复登录，同时让 Nobody 检查 API",
        "让 Alan 修复登录并让 Nobody 检查 API",
    ] {
        let manifest =
            build_original_target_manifest(&directory(), WorkspaceRoutingMode::Execute, body, &[]);
        assert!(
            !manifest.complete || manifest.targets.contains(&named("Nobody")),
            "重复命令中的未知第二目标不能从manifest消失: {body}"
        );
    }
}

// 场景：讨论出现在第一个角色称呼之后，而模型分类Execute；构造带角色的讨论
// 和一个直接任务正例，校验原文manifest；预期讨论零接收，直接任务仍可解析。
#[test]
fn discussion_after_recipient_cannot_be_upgraded_to_execution() {
    for body in [
        "Alan，我们讨论一下怎么分派任务",
        "@Alan 讨论一下派工方案",
        "@Alan 如何分派任务",
    ] {
        let manifest =
            build_original_target_manifest(&directory(), WorkspaceRoutingMode::Execute, body, &[]);
        let expected = WorkspaceResolution::Ready {
            body: body.into(),
            targets: vec![frozen("alpha", "first")],
        };
        assert_eq!(
            validate_workspace_task_plan(&manifest, &expected, &[frozen("alpha", "first")]),
            Err(WorkspacePlanError::AmbiguousOriginalTargets),
            "角色称呼不能赋予讨论原文执行权限: {body}"
        );
    }
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "让 Alan 修复登录",
        &[],
    );
    assert!(manifest.complete, "明确任务命令仍可通过原文门控");
}

// 场景：原文只有角色称呼和寒暄，模型却分类Execute；从文字/结构化@同一原文
// 提取manifest再校验计划；预期提及角色本身不证明执行命令，直接修复命令仍允许解析。
#[test]
fn recipient_mention_without_command_cannot_authorize_execution() {
    for body in ["@Alan 很不错", "Alan 辛苦了"] {
        let manifest =
            build_original_target_manifest(&directory(), WorkspaceRoutingMode::Execute, body, &[]);
        assert!(!manifest.complete, "称呼后的寒暄不赋予模型执行权限: {body}");
    }
    let mention = WorkspaceStructuredMention {
        start_byte: 0,
        end_byte: "@Alan".len(),
        card: card("first", "alpha"),
        project: None,
    };
    let manifest = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "@Alan 辛苦了",
        &[mention],
    );
    assert!(!manifest.complete, "结构化Card身份也不能把寒暄变成执行权限");
    let direct = build_original_target_manifest(
        &directory(),
        WorkspaceRoutingMode::Execute,
        "@Alan 修复登录",
        &[],
    );
    assert!(direct.complete, "明确修复命令仍可绑定角色");
}
