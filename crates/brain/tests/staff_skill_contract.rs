use brain::staff::policy::{authorize_staff, Verdict};
use norma::orchestration::brain_skill_source;
use norma::protocol::{BrainOp, StaffOp};
use std::path::Path;

fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let start = text.find(from).expect("section start");
    let end = text[start..].find(to).map_or(text.len(), |at| start + at);
    &text[start..end]
}

/// 场景：幕僚能执行的命令必须都写在 skill 的幕僚小节里；不能执行的不能出现在那里。
/// 数据构造：用 authorize_staff 判定的放行命令（对应的命令写法）与被拒绝的命令写法。
/// 预期结果：放行命令的写法都在幕僚小节；被拒绝命令的写法一条都不在；主大脑和审批小节仍有 decide。
#[test]
fn staff_section_lists_exactly_the_commands_staff_may_run() {
    let text = brain_skill_source(Path::new("/app/norma"));
    let staff = section(&text, "## Staff", "## Brain");
    let allowed = [
        ("`agents`", BrainOp::Agents),
        ("`approvals`", BrainOp::Approvals),
        ("`questions`", BrainOp::Questions),
        (
            "`history ",
            BrainOp::History {
                agent: "a".into(),
                last: None,
                grep: None,
            },
        ),
        (
            "`propose ",
            BrainOp::Propose {
                agent: "a".into(),
                title: "t".into(),
                task: "x".into(),
                why: "w".into(),
            },
        ),
        (
            "`task propose ",
            BrainOp::Task {
                op: StaffOp::Propose {
                    task_id: "a".into(),
                    agent: "s".into(),
                    title: "t".into(),
                    prompt: "p".into(),
                    why: "w".into(),
                },
            },
        ),
        (
            "`task assign ",
            BrainOp::Task {
                op: StaffOp::Assign {
                    task_id: "a".into(),
                    agent: "s".into(),
                    title: "t".into(),
                    prompt: "p".into(),
                    why: "w".into(),
                    worker: None,
                },
            },
        ),
        (
            "`task phase ",
            BrainOp::Task {
                op: StaffOp::Phase {
                    task_id: "a".into(),
                    phase: norma::protocol::TaskPhase::Done,
                },
            },
        ),
    ];
    for (written, op) in &allowed {
        assert_eq!(authorize_staff(op, true), Verdict::Allow, "{op:?}");
        assert!(
            staff.contains(written),
            "staff section must document {written}"
        );
    }
    for written in [
        "`report ",
        "`task archive ",
        "`propose-answer ",
        "`context ",
    ] {
        assert!(
            staff.contains(written),
            "staff section must document {written}"
        );
    }
    for denied in [
        "`decide ",
        "`answer ",
        "`assign ",
        "`task create",
        "`task list",
        "`task message",
        "`task reply",
    ] {
        assert!(
            !staff.contains(denied),
            "staff must not be told about {denied}"
        );
    }
    let brain = section(&text, "## Brain", "## Approval");
    assert!(
        brain.contains("`answer ") && brain.contains("`assign "),
        "check: brain.contains('`answer ') && brain.contains('`ass"
    );
    assert!(
        section(&text, "## Approval", "\u{0}").contains("`decide "),
        "check: section(&text, '## Approval', 'u(0)').contains('`"
    );
}
