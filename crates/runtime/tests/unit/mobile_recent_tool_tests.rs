use super::*;
use norma::agent_hook::{parse_hook_event, AgentHookEvent, HookSource};
use norma::mobile::RecentTool;
use serde_json::{json, Value};

fn hook(name: &str, turn: &str, mut raw: Value) -> AgentHookEvent {
    raw["hook_event_name"] = json!(name);
    raw["session_id"] = json!("tool-session");
    raw["turn_id"] = json!(turn);
    parse_hook_event(HookSource::Claude, 1, raw).expect("明确的原卡事件")
}

fn tool(name: &str, arg: Option<&str>) -> Option<RecentTool> {
    Some(RecentTool {
        name: name.into(),
        arg: arg.map(str::to_owned),
    })
}

/// 场景：岛的运行列表第三行展示最近一次工具。
/// 数据构造：同一回合先后出现带 description 的 Bash（命令正文含 SECRET）、编辑带目录的文件、无任何参数的工具。
/// 执行过程：提交提示词 → 依次 PreToolUse → PostToolUse。
/// 预期结果：Bash 取 description 且不含命令正文；文件工具只取 basename；无参数时只有工具名；PostToolUse 后仍保留。
#[test]
fn recent_tool_keeps_name_and_a_safe_argument_only() {
    let mut summary = summary::TaskSummary::default();
    summary.observe_hook(&hook("UserPromptSubmit", "a", json!({})));
    summary.observe_hook(&hook(
        "PreToolUse",
        "a",
        json!({"tool_name":"Bash","tool_input":{"description":"运行单元测试","command":"SECRET_COMMAND_BODY"}}),
    ));
    assert_eq!(summary.last_tool(), tool("Bash", Some("运行单元测试")));
    summary.observe_hook(&hook(
        "PreToolUse",
        "a",
        json!({"tool_name":"Edit","tool_input":{"file_path":"/Users/x/code/island/mod.rs"}}),
    ));
    assert_eq!(
        summary.last_tool(),
        tool("Edit", Some("mod.rs")),
        "只留文件名，不留目录"
    );
    summary.observe_hook(&hook(
        "PreToolUse",
        "a",
        json!({"tool_name":"Bash","tool_input":{"command":"SECRET_COMMAND_BODY"}}),
    ));
    assert_eq!(
        summary.last_tool(),
        tool("Bash", None),
        "没有 description 时不得退回命令正文"
    );
    summary.observe_hook(&hook("PostToolUse", "a", json!({"tool_name":"Bash"})));
    assert_eq!(
        summary.last_tool(),
        tool("Bash", None),
        "工具结束后仍是最近一次"
    );
}

/// 场景：新一轮输入或新会话开始后，上一轮的工具不能挂在新输入下面。
/// 预期结果：换回合 UserPromptSubmit 与 SessionStart 都清空；其他回合的工具事件被忽略；缺少工具名的事件不产生记录。
#[test]
fn recent_tool_is_scoped_to_the_current_turn() {
    let mut summary = summary::TaskSummary::default();
    summary.observe_hook(&hook("UserPromptSubmit", "a", json!({})));
    summary.observe_hook(&hook("PreToolUse", "a", json!({"tool_name":"Read"})));
    summary.observe_hook(&hook("PreToolUse", "old", json!({"tool_name":"Grep"})));
    assert_eq!(
        summary.last_tool(),
        tool("Read", None),
        "旧回合事件不得覆盖"
    );
    summary.observe_hook(&hook(
        "PreToolUse",
        "a",
        json!({"tool_input":{"description":"无名"}}),
    ));
    assert_eq!(summary.last_tool(), tool("Read", None), "缺工具名不改记录");
    summary.observe_hook(&hook("UserPromptSubmit", "b", json!({})));
    assert_eq!(summary.last_tool(), None, "新回合清空");
    summary.observe_hook(&hook("PreToolUse", "b", json!({"tool_name":"Write"})));
    summary.observe_hook(&hook("SessionStart", "b", json!({})));
    assert_eq!(summary.last_tool(), None, "新会话清空");
}

/// 场景：权限请求同样代表“正在做的工具”，且参数有长度上限。
#[test]
fn recent_tool_from_permission_request_is_bounded() {
    let mut summary = summary::TaskSummary::default();
    summary.observe_hook(&hook("UserPromptSubmit", "a", json!({})));
    summary.observe_hook(&hook(
        "PermissionRequest",
        "a",
        json!({"tool_name":"Bash","tool_input":{"description":"长".repeat(500)}}),
    ));
    let got = summary.last_tool().expect("权限请求也记录工具");
    assert_eq!(got.name, "Bash");
    assert!(
        got.arg.expect("有参数").chars().count() <= 240,
        "参数不得超过摘录上限"
    );
}
