//! Spike-backed stdout for Grok, Kimi, ZCode and InfCode question hooks.
use norma::agent_hook::ClaudeQuestion;
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionRef};
use serde_json::{json, Value};

fn raw(provider_tool: &str) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "hookEventName": "pre_tool_use",
        "tool_name": provider_tool,
        "toolName": provider_tool,
        "session_id": "sess-spike",
        "sessionId": "sess-spike",
        "tool_use_id": "call-spike",
        "toolUseId": "call-spike",
        "tool_input": {
            "questions": [{
                "question": "Spike color?",
                "header": "Color",
                "options": [
                    {"label": "Blue", "description": "Blue option"},
                    {"label": "Green", "description": "Green option"}
                ]
            }]
        }
    })
}

fn kimi_raw() -> Value {
    let mut raw = raw("AskUserQuestion");
    raw.as_object_mut().unwrap().remove("tool_use_id");
    raw.as_object_mut().unwrap().remove("toolUseId");
    raw["tool_call_id"] = json!("call-spike");
    raw
}

fn target(provider: &str, raw: &Value) -> QuestionRef {
    serde_json::from_value(json!({
        "workspace_id": "w",
        "project_id": "p",
        "node_id": 7,
        "provider": provider,
        "run_id": "run-1",
        "connection_id": "c",
        "session_id": raw["session_id"],
        "request_id": "call-spike",
        "revision": 1
    }))
    .unwrap()
}

fn blue(question: &norma::protocol::PendingQuestion) -> Vec<QuestionAnswer> {
    vec![QuestionAnswer {
        key: question.items[0].key.clone(),
        selected: vec![question.items[0].options[0].key.clone()],
        text: None,
    }]
}

/// 场景：Grok 岛内作答必须是顶层 deny，不能照抄 Claude 的 allow+updatedInput。
///
/// 数据构造：
///   工具名 ask_user_question（实机 ev-00），双 casing 同时存在
///   选项 Blue/Green，选择第 0 项 Blue
///
/// 执行过程：
///   1. from_profile(Grok) 解析 PreToolUse
///   2. answer_output 生成桥接 JSON
///   3. sanitize_hook_response 去掉 answers，得到 hook stdout
///
/// 预期结果：
///   - 正断言：stdout 为 decision=deny，reason 含 Blue
///   - 负断言：不得出现 permissionDecision=allow 或 updatedInput
#[test]
fn grok_answer_is_top_level_deny_without_updated_input() {
    let raw = raw("ask_user_question");
    let adapter = ClaudeQuestion::from_profile(AgentProfile::Grok, &raw).expect("Grok 问答可解析");
    let question = adapter.question(target("Grok", &raw)).unwrap();
    let bridge = adapter.answer_output(&blue(&question)).unwrap();
    let stdout = adapter.sanitize_hook_response(bridge).unwrap();
    assert_eq!(stdout["decision"], "deny", "Grok 实机只接受顶层 deny");
    assert_eq!(
        stdout["reason"], "The user answered Spike color? with Blue.",
        "reason 必须带上所选 label"
    );
    assert!(
        stdout.get("hookSpecificOutput").is_none(),
        "Grok 带 permissionDecision 的 hookSpecificOutput 不会改写问答"
    );
    assert!(
        stdout.get("updatedInput").is_none(),
        "updatedInput.answers 被 Grok 忽略，不能当答案通道"
    );
    assert!(
        stdout.get("answers").is_none(),
        "交给 Grok 的 stdout 不能残留桥接用的 answers"
    );
}

/// 场景：Kimi 只认 hookSpecificOutput deny，且调用 id 在 tool_call_id。
///
/// 数据构造：去掉 tool_use_id，只留 tool_call_id=call-spike；选择 Blue。
/// 执行：解析、生成 stdout。
/// 预期：permissionDecision=deny 且 reason 含 Blue；没有 updatedInput、顶层 decision，也不残留桥接 answers。
#[test]
fn kimi_answer_uses_permission_decision_reason() {
    let raw = kimi_raw();
    let adapter = ClaudeQuestion::from_profile(AgentProfile::Kimi, &raw).expect("Kimi 问答可解析");
    assert_eq!(
        adapter.tool_use_id(),
        "call-spike",
        "tool_call_id 即本题 id"
    );
    let question = adapter.question(target("Kimi", &raw)).unwrap();
    let stdout = adapter
        .sanitize_hook_response(adapter.answer_output(&blue(&question)).unwrap())
        .unwrap();
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecision"], "deny",
        "Kimi 实机 deny 在 hookSpecificOutput"
    );
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecisionReason"],
        "The user answered Spike color? with Blue.",
        "reason 进入 permissionDecisionReason"
    );
    assert!(
        stdout.get("decision").is_none(),
        "顶层 decision 没有在 Kimi 上单独验证，不能当唯一通道"
    );
    assert!(
        stdout["hookSpecificOutput"].get("updatedInput").is_none(),
        "Kimi 没有 updatedInput"
    );
    assert!(
        stdout.get("answers").is_none(),
        "交给 Kimi 的 stdout 不能残留桥接用的 answers"
    );
}

/// 场景：ZCode 走 Claude 式 allow，但必须保留原 questions。
///
/// 数据构造：工具名 AskUserQuestion，选择 Blue。
/// 执行：生成并消毒 hook stdout。
/// 预期：permissionDecision=allow，answers 为 Spike color?=Blue，questions 原文仍在。
#[test]
fn zcode_answer_rewrites_the_full_input() {
    let raw = raw("AskUserQuestion");
    let adapter =
        ClaudeQuestion::from_profile(AgentProfile::ZCode, &raw).expect("ZCode 问答可解析");
    let question = adapter.question(target("ZCode", &raw)).unwrap();
    let stdout = adapter
        .sanitize_hook_response(adapter.answer_output(&blue(&question)).unwrap())
        .unwrap();
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecision"], "allow",
        "ZCode 实机要 allow 才会执行工具"
    );
    assert_eq!(
        stdout["hookSpecificOutput"]["updatedInput"]["answers"],
        json!({"Spike color?": "Blue"}),
        "answers 键是问题全文"
    );
    assert_eq!(
        stdout["hookSpecificOutput"]["updatedInput"]["questions"], raw["tool_input"]["questions"],
        "updatedInput 是整份替换，原 questions 不能丢"
    );
    assert_eq!(stdout.get("decision"), None, "ZCode 不走顶层 deny");
}

/// 场景：InfCode 工具名是 question。deny reason 要同时出现在顶层和 hookSpecificOutput。
///
/// 数据构造：tool_name=question，选择 Blue。
/// 执行：AskUserQuestion 这个名字必须解析失败；question 成功并生成 stdout。
/// 预期：两处 reason 都含 Blue；AskUserQuestion 被拒绝。
#[test]
fn infcode_answer_denies_the_question_tool() {
    let wrong = raw("AskUserQuestion");
    assert!(
        ClaudeQuestion::from_profile(AgentProfile::InfCode, &wrong).is_err(),
        "InfCode 活工具不是 AskUserQuestion"
    );
    let raw = raw("question");
    let adapter =
        ClaudeQuestion::from_profile(AgentProfile::InfCode, &raw).expect("question 工具可解析");
    let question = adapter.question(target("InfCode", &raw)).unwrap();
    let stdout = adapter
        .sanitize_hook_response(adapter.answer_output(&blue(&question)).unwrap())
        .unwrap();
    let reason = "The user answered Spike color? with Blue.";
    assert_eq!(stdout["decision"], "deny", "顶层 deny 与实机 spy 一致");
    assert_eq!(stdout["reason"], reason, "顶层 reason 进入 tool error");
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecisionReason"], reason,
        "hookSpecificOutput 里的 reason 也要在"
    );
    assert!(
        stdout.get("answers").is_none(),
        "InfCode stdout 不携带桥接 answers"
    );
}

/// 场景：Hermes 与 Pi 没有可回传的问答工具。
///
/// 数据构造：同一份 AskUserQuestion 载荷。
/// 执行：按这两个 profile 解析。
/// 预期：两者都失败，不能登记成待答。
#[test]
fn hermes_and_pi_are_not_hook_question_providers() {
    let raw = raw("AskUserQuestion");
    assert!(
        ClaudeQuestion::from_profile(AgentProfile::Hermes, &raw).is_err(),
        "Hermes observer hook 不能回传答案"
    );
    assert!(
        ClaudeQuestion::from_profile(AgentProfile::Pi, &raw).is_err(),
        "Pi 没有内置问答工具"
    );
}
