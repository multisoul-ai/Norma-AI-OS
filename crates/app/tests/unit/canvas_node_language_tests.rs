use super::node_type_label;
use canvas::NodeType;
use ui::i18n::Locale;

/// 画布卡片的类型标签即时切换，技术名 Agent 保持原文。
///
/// 数据构造：Term、Browser、Preview、Rect 四类产品标签及 Agent 技术标识。
/// 执行过程：先应用中文读取五种标签，再切换英文读取终端标签，最后恢复英文。
/// 预期结果：中文显示终端/浏览器/预览/节点；Agent 保持原文；英文显示 TERMINAL，中文不能残留 TERMINAL。
#[test]
fn canvas_node_badges_follow_interface_language() {
    Locale::ZhCn.apply();
    assert_eq!(
        node_type_label(NodeType::Term),
        "终端",
        "终端类型标签必须使用中文"
    );
    assert_eq!(
        node_type_label(NodeType::Browser),
        "浏览器",
        "浏览器类型标签必须使用中文"
    );
    assert_eq!(
        node_type_label(NodeType::Preview),
        "预览",
        "预览类型标签必须使用中文"
    );
    assert_eq!(
        node_type_label(NodeType::Rect),
        "节点",
        "普通节点类型标签必须使用中文"
    );
    assert_eq!(
        node_type_label(NodeType::Agent),
        "Agent",
        "Agent 技术标识应保持原文"
    );
    assert_ne!(
        node_type_label(NodeType::Term),
        "TERMINAL",
        "中文终端不能残留英文类型标签"
    );
    Locale::En.apply();
    assert_eq!(
        node_type_label(NodeType::Term),
        "TERMINAL",
        "切换英文应恢复既有大写类型标签"
    );
}
