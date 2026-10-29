use norma::cli::parse_command_from;
use serde_json::json;

/// 场景：代答必须回到完整问题引用，不能转成新任务或把数字 RPC ID 改成字符串。
/// 构造：两张引用仅 request_id 类型不同；答案选择本地 option key green。
/// 执行：从公开 CLI 解析 answer 命令并序列化实际 CardCommand。
/// 预期：命令为 Brain.Answer，完整目标和答案原样保留，两种 ID 不合并。
#[test]
fn answer_cli_preserves_exact_question_identity() {
    for request_id in [json!(7), json!("7")] {
        let target = json!({
            "workspace_id":"workspace-a", "project_id":"project-a", "node_id":42,
            "provider":"Codex", "run_id":"run-1", "connection_id":"connection-1",
            "session_id":"thread-1", "turn_id":"turn-1", "request_id":request_id,
            "revision":1
        });
        let answers = json!([{"key":"label","selected":["green"]}]);
        let command = parse_command_from([
            "norma",
            "brain",
            "--token",
            "test-token",
            "answer",
            "--target",
            &target.to_string(),
            "--answers-json",
            &answers.to_string(),
        ])
        .expect("有效代答命令应能解析");
        assert_eq!(
            serde_json::to_value(command).unwrap(),
            json!({"Brain":{
                "token":"test-token", "op":{"Answer":{"target":target,"answers":answers}}
            }}),
            "回答必须保留原引用，不转换为 Assign/DispatchAgentTasks"
        );
    }
}

/// 场景：幕僚读问题和提出答案不得变成实际提交；无效目标在 CLI 边界就拒绝。
/// 构造：完整的 Claude 引用和仅含一个自由文本的答案，另构造空 run_id。
/// 执行：解析 questions/propose-answer，并将同引用改为空代数尝试 answer。
/// 预期：读取与建议有独立操作，建议保留理由，空代数解析失败。
#[test]
fn question_discovery_and_suggestion_are_distinct_from_delivery() {
    let command = parse_command_from(["norma", "brain", "--token", "test-token", "questions"])
        .expect("应支持查询待答问题");
    assert_eq!(
        serde_json::to_value(command).unwrap()["Brain"]["op"],
        json!("Questions"),
        "查询不得执行回答"
    );
    let mut target = json!({"workspace_id":"w", "project_id":"p", "node_id":42,
        "provider":"Claude", "run_id":"r", "connection_id":"c", "session_id":"s",
        "request_id":"tool-1", "revision":1});
    let command = parse_command_from([
        "norma",
        "brain",
        "--token",
        "test-token",
        "propose-answer",
        "--target",
        &target.to_string(),
        "--answers-json",
        "[{\"key\":\"0\",\"selected\":[],\"text\":\"用户原文\"}]",
        "--reason",
        "匹配用户要求",
    ])
    .expect("有效建议应能解析");
    assert_eq!(
        serde_json::to_value(command).unwrap()["Brain"]["op"]["ProposeAnswer"]["reason"],
        "匹配用户要求",
        "建议应保留理由且不伪装为 Answer"
    );
    target["run_id"] = json!("");
    assert!(
        parse_command_from([
            "norma",
            "brain",
            "answer",
            "--target",
            &target.to_string(),
            "--answers-json",
            "[]"
        ])
        .is_err(),
        "缺少运行代数不能回答"
    );
}
