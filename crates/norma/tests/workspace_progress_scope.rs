use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：全量授权范围覆盖首屏外项目和卡片，分页不截断总计；部分缺口与原观察时间明确输出。
/// 数据构造：4个授权项目各3张卡/任务（4×3=12），另有1个未授权秘密项目；第4项目成果读取失败。
/// 执行过程：输入全量事实，取得3项目首屏与第4项目2任务页，再收窄到第4项目查询。
/// 预期结果：首屏仍保留12总计和next；缺口只来自授权项目；未授权名称/数量/任务全不出现在结果。
#[test]
fn authorized_full_totals_survive_pagination_and_report_coverage_gaps() {
    let mut data = facts();
    data.scope.whole_workspace = false;
    data.scope.authorized_area_ids = (1..=4).map(|n| format!("area-{n}")).collect();
    data.areas.clear();
    data.cards.clear();
    for n in 1..=5 {
        let area = format!("area-{n}");
        data.areas.push(ProgressArea {
            workspace_id: "space-a".into(),
            area_id: area.clone(),
            project_id: format!("project-{n}"),
            project_name: if n == 5 {
                "未授权秘密项目".into()
            } else {
                format!("项目{n}")
            },
            presence: if n == 1 {
                AreaPresence::Foreground
            } else if n == 2 {
                AreaPresence::Background
            } else {
                AreaPresence::Unloaded
            },
        });
        for c in 1..=3 {
            let mut role = facts().cards.remove(0);
            role.card.area_id = area.clone();
            role.card.node_id = c;
            role.card.card_instance_id = format!("card-{n}-{c}");
            data.cards.push(role.clone());
            let mut row = task(&format!("task-{n}-{c}"), TaskPhase::Accepted, 100);
            row.card = role.card;
            data.tasks.push(row);
        }
        for facet in [
            CoverageFacet::Directory,
            CoverageFacet::Tasks,
            CoverageFacet::Activity,
            CoverageFacet::Outcomes,
        ] {
            data.coverage.push(ProgressCoverageFact {
                workspace_id: "space-a".into(),
                area_id: area.clone(),
                facet,
                state: if n == 4 && facet == CoverageFacet::Outcomes {
                    CoverageState::Missing
                } else {
                    CoverageState::Complete
                },
                observed_at_unix_ms: Some(123),
                reason: if n == 4 && facet == CoverageFacet::Outcomes {
                    Some("SQLite不可读".into())
                } else {
                    None
                },
            });
        }
    }
    let result = aggregate_workspace_progress(query(), data.clone())
        .expect("单区域读取失败应保留其他已知事实");
    assert_eq!(
        result.totals.project_count, 4,
        "只统计授权4项目，首屏外也必须计数"
    );
    assert_eq!(
        result.totals.role_count, 12,
        "全部4×3角色都计数，停用和未加载不排除"
    );
    assert_eq!(
        result.totals.default_report_task_count, 12,
        "全部授权任务都在默认当前概况"
    );
    assert!(
        !result.coverage_complete,
        "成果范围读取失败必须如实标部分结果"
    );
    assert_eq!(
        result.coverage.len(),
        16,
        "授权4区域×4类范围，未授权缺口也不可泄漏"
    );
    assert_eq!(
        result.coverage[0].observed_at_unix_ms,
        Some(123),
        "覆盖采样观察时间不得替换为查询时间"
    );
    let encoded = serde_json::to_string(&result).expect("进度快照应可序列化");
    assert!(
        !encoded.contains("未授权秘密项目")
            && !encoded.contains("task-5")
            && !encoded.contains("card-5"),
        "授权前过滤不得泄漏范围外名称或内容"
    );
    let page = result.project_page(0, 3);
    assert_eq!(page.items.len(), 3, "项目详情首屏可有界展示");
    assert_eq!(page.total_items, 4, "分页总数来自全量已授权项目");
    assert_eq!(page.next_offset, Some(3), "首屏外第4项目有稳定后续页");
    assert_eq!(page.totals.role_count, 12, "详情截断不能改变完整空间总计");
    let tasks = result.projects[3].task_page(0, 2);
    assert_eq!(tasks.items.len(), 2, "单项目任务页可有界");
    assert_eq!(tasks.total_items, 3, "第3张卡的任务仍计入该项目总数");
    let mut claimed_whole = data.clone();
    claimed_whole.scope.whole_workspace = true;
    let declared =
        aggregate_workspace_progress(query(), claimed_whole).expect("范围声明不应影响allowlist");
    assert_eq!(
        declared.totals.role_count, 12,
        "whole_workspace=true也不能扩大明确authorized_area_ids"
    );
    let narrowed = aggregate_workspace_progress(
        ProgressQuery {
            project_id: Some("project-4".into()),
            ..query()
        },
        data,
    )
    .expect("项目筛选只能收窄授权范围");
    assert_eq!(narrowed.totals.project_count, 1, "指定项目不会引入其他项目");
    assert_eq!(narrowed.totals.role_count, 3, "指定项目只统计其授权角色");
    assert!(
        !narrowed.scope.whole_workspace,
        "授权范围限制不能被筛选伪装成完整全空间"
    );
}

/// 场景：区域目录元数据读取失败时，仍保留该授权区域已有的卡片/任务事实，不能静默丢失或猜项目。
/// 数据构造：授权area-missing没有ProgressArea元数据，但有1张保留卡和1个运行任务；原area-a仍可读。
/// 执行过程：聚合完整授权范围，再按已知project-a收窄查询。
/// 预期结果：全范围保留角色/任务并放入明确归属待核对分组，缺口明确；项目筛选不把未知归属任务猜进已知项目。
#[test]
fn missing_area_metadata_preserves_authorized_known_work_without_guessing_project() {
    let mut data = facts();
    data.scope.authorized_area_ids.push("area-missing".into());
    data.cards[0].card.area_id = "area-missing".into();
    let mut pending = task("known-task", TaskPhase::Running, 100);
    pending.card.area_id = "area-missing".into();
    data.tasks.push(pending);
    let result = aggregate_workspace_progress(query(), data.clone())
        .expect("目录部分失败仍应保留已知任务事实");
    assert_eq!(
        result.totals.role_count, 1,
        "已授权卡片不能因区域目录缺失消失"
    );
    assert_eq!(
        result.totals.all_known_logical_task_count, 1,
        "已知运行任务不能静默丢失"
    );
    assert_eq!(
        result.totals.project_count, 1,
        "只能计1个确证项目，不能为未知归属编造第2项目"
    );
    let unknown = result
        .projects
        .iter()
        .find(|p| !p.project_identity_known)
        .expect("必须单列项目归属待核对的工作");
    assert_eq!(
        unknown.areas[0].presence,
        AreaPresence::Unknown,
        "缺目录时不能猜区域已加载或未加载"
    );
    assert_eq!(
        unknown.tasks[0].card.area_id, "area-missing",
        "原区域身份必须保留"
    );
    assert_eq!(
        unknown.tasks[0].phase,
        TaskPhase::Running,
        "项目目录缺失不丢失已知阶段证据"
    );
    assert!(!result.coverage_complete, "目录缺失不能报告完整覆盖");
    let narrowed = aggregate_workspace_progress(
        ProgressQuery {
            project_id: Some("project-a".into()),
            ..query()
        },
        data,
    )
    .expect("指定项目可返回带缺口部分结果");
    assert_eq!(
        narrowed.totals.all_known_logical_task_count, 0,
        "不得凭缺失目录猜任务归已知项目"
    );
    assert!(
        !narrowed.coverage_complete,
        "不能因项目筛选掩盖未知区域可能属于该项目的缺口"
    );
}
