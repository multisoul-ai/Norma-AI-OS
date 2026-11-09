use norma::agent_hook::{
    extract_prompt_text, prompt_is_only_settled_task_notification,
    user_prompt_is_only_settled_task_notification,
};
use serde_json::json;

/// Claude 的 task-notification 自动回合不能被当成新的用户根任务。
///
/// 数据构造：
///   纯通知 = 一个 completed 的 `<task-notification>`，没有其它正文。
///   用户句子 = "继续重构"，以及夹了一段通知但仍有正文的 prompt。
///   running 通知没有结清 id，不能当成已结束回合。
///
/// 执行过程：对三种 prompt 调用 `prompt_is_only_settled_task_notification`。
///
/// 预期结果：
///   - 正断言：只有纯 settled 通知返回 true。
///   - 负断言：用户句子、夹杂正文、以及 running 通知都返回 false。
#[test]
fn only_settled_task_notification_prompts_skip_root_registration() {
    let notification = "<task-notification><tool-use-id>toolu_1</tool-use-id><status>completed</status></task-notification>";
    let mixed = "继续重构\n<task-notification><task-id>t1</task-id><status>completed</status></task-notification>";
    let running =
        "<task-notification><task-id>t1</task-id><status>running</status></task-notification>";

    assert!(
        prompt_is_only_settled_task_notification(notification),
        "纯 completed 通知必须跳过根任务登记"
    );
    assert!(
        !prompt_is_only_settled_task_notification("继续重构"),
        "用户句子必须继续登记根任务"
    );
    assert!(
        !prompt_is_only_settled_task_notification(mixed),
        "通知后面仍有用户正文时不得整段跳过"
    );
    assert!(
        !prompt_is_only_settled_task_notification(running),
        "running 通知没有结清，不得当成自动回合跳过"
    );
    let mixed_status = "<task-notification><task-id>done</task-id><status>completed</status></task-notification><task-notification><task-id>open</task-id><status>running</status></task-notification>";
    assert!(
        !prompt_is_only_settled_task_notification(mixed_status),
        "夹着 running 通知时不得整段跳过"
    );
}

/// 标题截断到 60 字后会丢掉 `</task-notification>`，跳过判断必须看原文。
///
/// 数据构造：
///   纯 completed 通知长度 77 字符，大于标题上限 60。
///   raw_json.prompt 放这份原文。
///
/// 执行过程：
///   1. 用 extract_prompt_text 得到截断标题。
///   2. 用 user_prompt_is_only_settled_task_notification 看原文。
///
/// 预期结果：
///   - 正断言：原文判断为 true，登记根任务时必须跳过。
///   - 负断言：截断标题自己不能判成已结清通知。
#[test]
fn long_settled_notification_is_skipped_from_the_raw_prompt() {
    let notification = "<task-notification><tool-use-id>toolu_1</tool-use-id><status>completed</status></task-notification>";
    let raw = json!({ "prompt": notification });
    let title = extract_prompt_text(&raw).expect("通知 prompt 应能取出标题");

    assert!(
        notification.chars().count() > 60,
        "这条例子必须长过 60 字标题上限，实际 {}",
        notification.chars().count()
    );
    assert!(
        user_prompt_is_only_settled_task_notification(&raw),
        "未截断的纯通知必须跳过根任务登记"
    );
    assert!(
        !prompt_is_only_settled_task_notification(&title),
        "60 字截断标题不得单独当成已结清通知，实际 {title}"
    );
}
