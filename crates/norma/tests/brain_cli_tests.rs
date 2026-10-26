use norma::cli::{parse_invocation_from, OutputFormat};
use norma::protocol::{BrainDecision, BrainOp, CardCommand, ReportPriority};

fn parse(args: &[&str]) -> CardCommand {
    parse_invocation_from(args.iter().copied())
        .unwrap_or_else(|error| panic!("参数 {args:?} 应能解析：{error}"))
        .command
}

/// `norma brain report` 解析成带优先级的播报命令。
///
/// 数据构造：`--token t0` 代替环境变量；文本“Codex 完成了”；分别用默认与 `--priority now`。
/// 预期结果：
///   - 正断言：默认优先级是 Normal，指定 now 为 Now；token 原样带上
///   - 负断言：文本不被截断或改写
#[test]
fn report_parses_text_and_priority() {
    let normal = parse(&[
        "norma",
        "brain",
        "--token",
        "t0",
        "report",
        "--text",
        "Codex 完成了",
    ]);
    assert_eq!(
        normal,
        CardCommand::Brain {
            token: "t0".into(),
            op: BrainOp::Report {
                text: "Codex 完成了".into(),
                sources: Vec::new(),
                priority: ReportPriority::Normal
            },
        },
        "默认优先级应是 Normal"
    );
    let now = parse(&[
        "norma",
        "brain",
        "--token",
        "t0",
        "report",
        "--text",
        "x",
        "--priority",
        "now",
    ]);
    assert!(
        matches!(
            now,
            CardCommand::Brain {
                op: BrainOp::Report {
                    priority: ReportPriority::Now,
                    ..
                },
                ..
            }
        ),
        "--priority now 应解析为 Now：{now:?}"
    );
}

/// `norma brain decide` 解析审批标识、结论和理由。
///
/// 数据构造：审批标识 `7.Claude.run-1.sess-1.turn-1.req-1`；三种结论 allow、deny、escalate。
/// 预期结果：
///   - 正断言：三种结论分别对应 BrainDecision 的三个值；理由原样保留
///   - 负断言：未知结论（如 maybe）解析失败；缺少理由也失败
#[test]
fn decide_parses_the_three_decisions_and_requires_a_reason() {
    let id = "7.Claude.run-1.sess-1.turn-1.req-1";
    for (word, want) in [
        ("allow", BrainDecision::Allow),
        ("deny", BrainDecision::Deny),
        ("escalate", BrainDecision::Escalate),
    ] {
        let command = parse(&[
            "norma",
            "brain",
            "--token",
            "t",
            "decide",
            id,
            word,
            "--reason",
            "测试命令",
        ]);
        assert_eq!(
            command,
            CardCommand::Brain {
                token: "t".into(),
                op: BrainOp::Decide {
                    approval_id: id.into(),
                    decision: want,
                    reason: "测试命令".into()
                },
            },
            "{word} 应解析为对应结论"
        );
    }
    assert!(
        parse_invocation_from([
            "norma", "brain", "--token", "t", "decide", id, "maybe", "--reason", "x"
        ])
        .is_err(),
        "未知结论应失败"
    );
    assert!(
        parse_invocation_from(["norma", "brain", "--token", "t", "decide", id, "allow"]).is_err(),
        "缺少理由应失败"
    );
}

/// agents、approvals、assign 子命令。
///
/// 预期结果：
///   - 正断言：agents 与 approvals 解析为对应操作；assign 带上 Agent 名和任务文字
///   - 负断言：任务文字为空白时解析失败（与现有 `agent task assign` 一致）
#[test]
fn agents_approvals_and_assign_parse() {
    assert!(matches!(
        parse(&["norma", "brain", "--token", "t", "agents"]),
        CardCommand::Brain {
            op: BrainOp::Agents,
            ..
        }
    ));
    assert!(matches!(
        parse(&["norma", "brain", "--token", "t", "approvals"]),
        CardCommand::Brain {
            op: BrainOp::Approvals,
            ..
        }
    ));
    let assign = parse(&[
        "norma",
        "brain",
        "--token",
        "t",
        "assign",
        "Cursor",
        "--task",
        "实现安全闸门",
    ]);
    assert_eq!(
        assign,
        CardCommand::Brain {
            token: "t".into(),
            op: BrainOp::Assign {
                agent: "Cursor".into(),
                task: "实现安全闸门".into()
            },
        },
        "assign 应带上 Agent 与任务"
    );
    assert!(
        parse_invocation_from([
            "norma", "brain", "--token", "t", "assign", "Cursor", "--task", "  "
        ])
        .is_err(),
        "空白任务应失败"
    );
}

/// brain 命令默认输出 JSON。
///
/// 预期结果：format 为 Json；没有目标 Workspace、没有 operation_id。
#[test]
fn brain_commands_default_to_json_output() {
    let invocation = parse_invocation_from(["norma", "brain", "--token", "t", "agents"]).unwrap();
    assert_eq!(invocation.format, OutputFormat::Json, "大脑读取 JSON");
    assert!(
        invocation.target_workspace_id.is_none() && invocation.operation_id.is_none(),
        "不带跨 Workspace 目标"
    );
}

/// `norma brain history` 带上 Agent 名、轮数和关键词。
///
/// 数据构造：一次给全参数，一次只给 Agent 名。
/// 预期结果：
///   - 正断言：全参数时 last=3、grep 原样；只给名字时 last 与 grep 都是 None
///   - 负断言：不带 --last 时不会被填成某个默认数，默认值由 App 决定
#[test]
fn history_parses_agent_last_and_grep() {
    let full = parse(&[
        "norma", "brain", "--token", "t0", "history", "Alan", "--last", "3", "--grep", "error",
    ]);
    assert_eq!(
        full,
        CardCommand::Brain {
            token: "t0".into(),
            op: BrainOp::History {
                agent: "Alan".into(),
                last: Some(3),
                grep: Some("error".into())
            },
        }
    );
    let bare = parse(&["norma", "brain", "--token", "t0", "history", "Alan"]);
    assert_eq!(
        bare,
        CardCommand::Brain {
            token: "t0".into(),
            op: BrainOp::History {
                agent: "Alan".into(),
                last: None,
                grep: None
            },
        }
    );
}

/// `norma brain propose` 解析成待用户确认的下一步。
///
/// 数据构造：给 Alan 提议“补测试”，带完整指令与理由；另一次缺 `--why`，一次 `--task` 只有空白。
/// 预期结果：
///   - 正断言：agent、title、task、why 原样带上
///   - 负断言：缺理由或指令为空白时解析失败，不会生成一条空提议
#[test]
fn propose_parses_title_task_and_why() {
    let command = parse(&[
        "norma",
        "brain",
        "--token",
        "t0",
        "propose",
        "Alan",
        "--title",
        "补测试",
        "--task",
        "为概览搜索补单元测试；cargo test 通过即完成",
        "--why",
        "搜索没有测试",
    ]);
    assert_eq!(
        command,
        CardCommand::Brain {
            token: "t0".into(),
            op: BrainOp::Propose {
                agent: "Alan".into(),
                title: "补测试".into(),
                task: "为概览搜索补单元测试；cargo test 通过即完成".into(),
                why: "搜索没有测试".into(),
            },
        }
    );
    let missing_why = parse_invocation_from([
        "norma", "brain", "propose", "Alan", "--title", "t", "--task", "x",
    ]);
    assert!(missing_why.is_err(), "缺 --why 应解析失败");
    let blank_task = parse_invocation_from([
        "norma", "brain", "propose", "Alan", "--title", "t", "--task", "  ", "--why", "w",
    ]);
    assert!(blank_task.is_err(), "空白指令应解析失败");
}
