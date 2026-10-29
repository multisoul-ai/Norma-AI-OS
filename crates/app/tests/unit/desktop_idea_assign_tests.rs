use super::*;

fn option(area: &str, node: u64) -> IdeaTargetOption {
    IdeaTargetOption {
        area_id: area.to_owned(),
        area_name: area.to_owned(),
        node_id: node,
        name: format!("agent-{node}"),
        status: "待开始",
        running: false,
    }
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

fn assert_bilingual_notice(plan: AssignPlan, chinese: &str, english: &str) {
    let AssignPlan::Notice(notice) = plan else {
        panic!("失败路径应返回提示，不能派发或打开选择器：{plan:?}");
    };
    assert_eq!(
        notice.resolve_for(ui::i18n::Locale::ZhCn),
        chinese,
        "中文提示应点明失败原因"
    );
    assert_eq!(
        notice.resolve_for(ui::i18n::Locale::En),
        english,
        "同一缓存提示应支持英文，不能保留硬编码中文"
    );
}

/// `⌃N` 取展开项目的第 N 张卡片，忽略其他项目的卡片；光标在待办行时派该行。
///
/// 数据构造：待办 [a, b, c]；项目 p1 有卡片 10、11，项目 p2 有卡片 20、21、22（顺序在 p1 之间穿插）；展开 p2。
/// 执行过程：光标在第 2 行，请求第 2 张卡片。
/// 预期结果：派发想法 b 给 p2 的 21，而不是 p1 的 11（负断言：不跨项目计数）。
#[test]
fn card_number_counts_within_expanded_area_and_uses_cursor_row() {
    let options = [
        option("p1", 10),
        option("p2", 20),
        option("p1", 11),
        option("p2", 21),
        option("p2", 22),
    ];
    let plan = plan_assignment(
        AssignRequest::Card(2),
        &ids(&["a", "b", "c"]),
        Some(1),
        &options,
        Some("p2"),
        AgentProfile::Codex,
    );
    assert_eq!(
        plan,
        AssignPlan::Dispatch {
            idea_id: "b".to_owned(),
            target: IdeaUiTarget::Existing {
                area_id: "p2".to_owned(),
                node_id: 21
            },
        },
        "应派想法 b 给 p2 的第 2 张卡片"
    );
}

/// 没有光标或光标在已指派行（超出待办范围）时派待办队首。
///
/// 数据构造：待办 [a, b]；光标分别为 None 与 Some(5)（在已指派区）。
/// 预期结果：两种情况都派 a。
#[test]
fn falls_back_to_first_todo_without_a_todo_cursor() {
    let options = [option("p1", 10)];
    for cursor in [None, Some(5)] {
        let plan = plan_assignment(
            AssignRequest::Card(1),
            &ids(&["a", "b"]),
            cursor,
            &options,
            Some("p1"),
            AgentProfile::Codex,
        );
        assert!(
            matches!(&plan, AssignPlan::Dispatch { idea_id, .. } if idea_id == "a"),
            "光标 {cursor:?} 应派队首 a，实际 {plan:?}"
        );
    }
}

/// 失败路径都给出可读提示，而不是静默忽略。
///
/// 数据构造：分别构造无待办、未展开项目、卡片序号越界（0 与 3，项目只有 1 张）。
/// 执行过程：生成派发计划，再将同一缓存提示分别解析为中文与英文。
/// 预期结果：全部为 Notice 而非派发或选择器，双语文案点明原因并保留卡片序号。
#[test]
fn failures_explain_themselves() {
    let options = [option("p1", 10)];
    let none = plan_assignment(
        AssignRequest::Card(1),
        &[],
        None,
        &options,
        Some("p1"),
        AgentProfile::Codex,
    );
    assert_bilingual_notice(none, "没有待办想法可派发", "No ideas to assign");
    let collapsed = plan_assignment(
        AssignRequest::NewAgent(0),
        &ids(&["a"]),
        None,
        &options,
        None,
        AgentProfile::Codex,
    );
    assert_bilingual_notice(
        collapsed,
        "先展开一个项目，再派发想法",
        "Expand a project before assigning ideas",
    );
    for number in [0, 3] {
        let plan = plan_assignment(
            AssignRequest::Card(number),
            &ids(&["a"]),
            None,
            &options,
            Some("p1"),
            AgentProfile::Codex,
        );
        assert_bilingual_notice(
            plan,
            &format!("当前项目没有第 {number} 张智能体卡片"),
            &format!("This project has no agent card {number}"),
        );
    }
}

/// 新建智能体：槽位 2 固定 Claude，槽位 0 与未绑定槽位用默认类型。
///
/// 数据构造：默认类型设为 Pi。
/// 预期结果：槽位 2 → Claude；槽位 0、1 → Pi；目标项目都是展开的 p1。
#[test]
fn new_agent_uses_slot_profile_or_default() {
    let cases = [
        (2, AgentProfile::Claude),
        (0, AgentProfile::Pi),
        (1, AgentProfile::Pi),
    ];
    for (slot, expected) in cases {
        let plan = plan_assignment(
            AssignRequest::NewAgent(slot),
            &ids(&["a"]),
            None,
            &[],
            Some("p1"),
            AgentProfile::Pi,
        );
        assert_eq!(
            plan,
            AssignPlan::Dispatch {
                idea_id: "a".to_owned(),
                target: IdeaUiTarget::New {
                    area_id: "p1".to_owned(),
                    profile: expected
                },
            },
            "槽位 {slot} 的类型"
        );
    }
}

/// `⌃⇧0` 打开类型选择器：校验有待办且有展开项目，否则给出提示而不是打开空浮层。
///
/// 预期结果：正常 → OpenPicker；没有待办 → 提示；没展开项目 → 提示。选定类型（含无数字键的 ZCode）→ 派发到新建卡片。
#[test]
fn picker_opens_only_when_dispatchable_and_accepts_any_profile() {
    let todo = ids(&["a"]);
    let plan = |req, todo: &[String], area| {
        plan_assignment(req, todo, None, &[], area, AgentProfile::Codex)
    };
    assert_eq!(
        plan(AssignRequest::Picker, &todo, Some("p")),
        AssignPlan::OpenPicker
    );
    assert!(matches!(
        plan(AssignRequest::Picker, &[], Some("p")),
        AssignPlan::Notice(_)
    ));
    assert!(matches!(
        plan(AssignRequest::Picker, &todo, None),
        AssignPlan::Notice(_)
    ));
    assert_eq!(
        plan(
            AssignRequest::Profile(AgentProfile::ZCode),
            &todo,
            Some("p")
        ),
        AssignPlan::Dispatch {
            idea_id: "a".to_owned(),
            target: IdeaUiTarget::New {
                area_id: "p".to_owned(),
                profile: AgentProfile::ZCode
            },
        }
    );
}

/// 槽位 1（`⌃⇧0` 的命令）是选择器，其余槽位仍是直接新建。
#[test]
fn picker_slot_maps_to_picker_request() {
    use crate::shortcuts::ShortcutCommandId as C;
    assert_eq!(
        request_for(C::AssignIdeaToNewAgent(1)),
        Some(AssignRequest::Picker)
    );
    assert_eq!(
        request_for(C::AssignIdeaToNewAgent(3)),
        Some(AssignRequest::NewAgent(3))
    );
}
