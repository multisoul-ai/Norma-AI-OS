use super::{
    format_create_ok, format_focus_ok, format_operation_failed, format_send_text_ok,
    template_report_for_error, template_report_for_result,
};
use norma::protocol::CardCommandResult;

/// DCT-1: Create 成功模板应包含 node_id。
///
/// 数据构造：
///   node_id = 42
///
/// 执行过程：
///   1. format_create_ok(42)
///
/// 预期结果：
///   - 正断言：文案为「已创建卡片 42」
#[test]
fn test_format_create_ok_includes_node_id() {
    assert_eq!(
        format_create_ok(42),
        "已创建卡片 42",
        "Create 成功模板应包含 node_id"
    );
}

/// DCT-2: SendText 成功模板应包含 node_id。
#[test]
fn test_format_send_text_ok_includes_node_id() {
    assert_eq!(
        format_send_text_ok(9),
        "已发送到卡片 9",
        "SendText 成功模板应包含 node_id"
    );
}

/// DCT-3: Focus 成功模板应包含 node_id。
#[test]
fn test_format_focus_ok_includes_node_id() {
    assert_eq!(
        format_focus_ok(5),
        "已聚焦卡片 5",
        "Focus 成功模板应包含 node_id"
    );
}

/// DCT-4: 失败模板应包含错误信息前缀。
#[test]
fn test_format_operation_failed_prefixes_message() {
    assert_eq!(
        format_operation_failed("节点不存在"),
        "操作失败：节点不存在",
        "失败模板应使用中文前缀"
    );
    assert_eq!(
        template_report_for_error("节点不存在"),
        "操作失败：节点不存在",
        "template_report_for_error 应复用失败模板"
    );
}

/// DCT-5: template_report_for_result 仅映射 Create/SendText/Focus 成功结果。
///
/// 预期结果：
///   - 正断言：Created/Sent/Focused 返回 Some
///   - 负断言：Listed/Deleted 返回 None
#[test]
fn test_template_report_for_result_maps_success_outcomes_only() {
    assert_eq!(
        template_report_for_result(&CardCommandResult::Created { node_id: 1 }),
        Some("已创建卡片 1".into()),
        "Created 应映射模板"
    );
    assert_eq!(
        template_report_for_result(&CardCommandResult::Sent { node_id: 2 }),
        Some("已发送到卡片 2".into()),
        "Sent 应映射模板"
    );
    assert_eq!(
        template_report_for_result(&CardCommandResult::Focused { node_id: 3 }),
        Some("已聚焦卡片 3".into()),
        "Focused 应映射模板"
    );
    assert!(
        template_report_for_result(&CardCommandResult::Listed { cards: vec![] }).is_none(),
        "Listed 不应生成语音模板"
    );
    assert!(
        template_report_for_result(&CardCommandResult::Deleted { node_id: 4 }).is_none(),
        "Deleted 不应生成语音模板"
    );
}
