use super::card_for;
use canvas::{BrowserSpec, CanvasNode, CanvasRect, NodeKind, PreviewSpec, RectNode};
use runtime::Runtime;
use ui::i18n::Locale;

fn title(kind: NodeKind) -> String {
    let node = CanvasNode {
        card_instance_id: canvas::new_card_instance_id(),
        id: 1, // 单节点，无 session 或其他节点；id 只用于 Card trait 路由。
        kind,
        bounds: CanvasRect::new(0.0, 0.0, 300.0, 200.0), // 300×200 不参与标题选择。
        z_index: 0,
        selected: false,
    };
    card_for(node.kind.node_type())
        .chrome(&node, &Runtime::new())
        .title
}

fn browser(name: Option<&str>, url: &str) -> NodeKind {
    NodeKind::Browser(BrowserSpec {
        title: name.map(str::to_string),
        url: url.into(),
        surface: Default::default(),
    })
}

fn preview(path: &str) -> NodeKind {
    NodeKind::Preview(PreviewSpec {
        path: path.into(),
        additional_paths: Vec::new(),
        explicit_type: None,
    })
}

/// 默认卡片标题跟随界面语言，恰好与默认标题相同的用户标题仍保留原文。
///
/// 数据构造：无标题无网址 Browser、无文件名 Preview、Rect；用户标题 Browser 和文件名 Preview。
/// 执行过程：应用中文 → 渲染三种默认标题 → 渲染用户标题/文件名/网址主机 → 切英文重渲染。
/// 预期结果：默认中文为浏览器/预览/节点；用户 Browser、Preview 和主机保持；中文默认不残留 Browser。
#[test]
fn card_default_titles_follow_locale_and_user_titles_are_preserved() {
    Locale::ZhCn.apply();
    assert_eq!(
        title(browser(None, "")),
        "浏览器",
        "缺省浏览器标题应使用中文"
    );
    assert_eq!(title(preview("/")), "预览", "无文件名预览应使用中文");
    assert_eq!(
        title(NodeKind::Rect(RectNode { color: 0 })),
        "节点",
        "普通节点应使用中文"
    );
    assert_eq!(
        title(browser(Some("Browser"), "")),
        "Browser",
        "用户标题不能被字符串匹配翻译"
    );
    assert_eq!(
        title(preview("/tmp/Preview")),
        "Preview",
        "用户文件名必须原样展示"
    );
    assert_eq!(
        title(browser(None, "https://example.com/page")),
        "example.com",
        "网页主机必须保留原文"
    );
    assert_ne!(
        title(browser(None, "")),
        "Browser",
        "中文默认浏览器不能残留英文"
    );
    Locale::En.apply();
    assert_eq!(
        title(browser(None, "")),
        "Browser",
        "英文应恢复默认浏览器标题"
    );
}
