use norma::agent_hook::{session_context_output, HookSource, PATH_CONVENTION_PROMPT};
use serde_json::{json, Value};

/// Claude/Codex 的 SessionStart 必须回传 additionalContext，内容为文件路径输出约定。
///
/// 数据构造：hook_event_name = "SessionStart"，source 分别为 startup 与 compact；`norma_managed = true`。
/// 执行过程：对 Claude 与 Codex 各调用一次 `session_context_output`，把输出解析为 JSON。
/// 预期结果：
///   - `hookSpecificOutput.hookEventName == "SessionStart"`；
///   - `additionalContext` 等于 `PATH_CONVENTION_PROMPT`；
///   - compact 后也要重新注入，否则压缩会让规则丢失。
#[test]
fn test_session_start_injects_path_convention_for_claude_and_codex() {
    for source in [HookSource::Claude, HookSource::Codex] {
        for start in ["startup", "compact"] {
            let raw = json!({"hook_event_name": "SessionStart", "source": start});
            let out = session_context_output(source, &raw, true, &mut |_| true)
                .unwrap_or_else(|| panic!("{source:?}/{start} 应输出上下文"));
            let value: Value = serde_json::from_str(&out).expect("stdout 必须是纯 JSON");
            assert_eq!(value["hookSpecificOutput"]["hookEventName"], "SessionStart");
            assert_eq!(
                value["hookSpecificOutput"]["additionalContext"], PATH_CONVENTION_PROMPT,
                "additionalContext 应是约定全文"
            );
        }
    }
}

/// 非 SessionStart、非 Norma 托管会话、未验证的 provider 都不得向 stdout 输出，避免污染 Agent。
///
/// 数据构造：UserPromptSubmit 事件；SessionStart 但 `norma_managed = false`；Cursor 的 SessionStart。
/// 预期结果：三种情况均返回 None。
#[test]
fn test_no_injection_outside_managed_session_start() {
    let start = json!({"hook_event_name": "SessionStart"});
    let prompt = json!({"hook_event_name": "UserPromptSubmit"});
    assert!(session_context_output(HookSource::Claude, &prompt, true, &mut |_| true).is_none());
    assert!(session_context_output(HookSource::Claude, &start, false, &mut |_| true).is_none());
}

/// 约定文案必须包含解析端依赖的关键规则，且体量受控（<= 900 字节，约 100+ token）。
#[test]
fn test_prompt_states_the_rules_the_parser_relies_on() {
    for needle in ["inline code", ":line", "relative", "absolute", "**bold**"] {
        assert!(
            PATH_CONVENTION_PROMPT.contains(needle),
            "缺少规则: {needle}"
        );
    }
    assert!(
        PATH_CONVENTION_PROMPT.len() <= 900,
        "约定过长会长期占用上下文"
    );
}

/// Cursor 用顶层 additional_context（camelCase 事件名）；Kimi 只在首条 UserPromptSubmit 输出纯文本。
///
/// 数据构造：Cursor sessionStart；Kimi UserPromptSubmit 带 session_id，claim 闭包第一次 true、之后 false；
///   Kimi 的 SessionStart（其 stdout 不入上下文，探针已验证）。
/// 预期结果：Cursor 输出 JSON；Kimi 首次输出约定全文、第二次 None；Kimi SessionStart 恒为 None。
#[test]
fn test_cursor_json_and_kimi_first_prompt_only() {
    let cursor = json!({"hook_event_name": "sessionStart"});
    let out = session_context_output(HookSource::Cursor, &cursor, true, &mut |_| true).unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["additional_context"], PATH_CONVENTION_PROMPT);

    let prompt = json!({"hook_event_name": "UserPromptSubmit", "session_id": "s1"});
    let mut seen = false;
    let mut claim = |_: &str| !std::mem::replace(&mut seen, true);
    assert_eq!(
        session_context_output(HookSource::Kimi, &prompt, true, &mut claim).as_deref(),
        Some(PATH_CONVENTION_PROMPT),
        "首条提示词应注入纯文本"
    );
    assert!(
        session_context_output(HookSource::Kimi, &prompt, true, &mut claim).is_none(),
        "同一会话第二条提示词不得重复注入"
    );
    let start = json!({"hook_event_name": "SessionStart", "session_id": "s1"});
    assert!(session_context_output(HookSource::Kimi, &start, true, &mut |_| true).is_none());
}

/// `NORMA_PATH_CONVENTION=0` 关闭注入；未设置或其他值保持开启。
///
/// 本文件内只有这一个测试读写该环境变量，不会与其他测试竞争。
/// 预期结果：未设置 → true；"0" → false；"1" → true。
#[test]
fn test_env_switch_disables_injection() {
    use norma::agent_hook::{path_convention_enabled_from_env, ENV_PATH_CONVENTION};
    std::env::remove_var(ENV_PATH_CONVENTION);
    assert!(path_convention_enabled_from_env(), "未设置默认开启");
    std::env::set_var(ENV_PATH_CONVENTION, "0");
    assert!(!path_convention_enabled_from_env(), "值为 0 应关闭");
    std::env::set_var(ENV_PATH_CONVENTION, "1");
    assert!(path_convention_enabled_from_env(), "其他值保持开启");
    std::env::remove_var(ENV_PATH_CONVENTION);
}
