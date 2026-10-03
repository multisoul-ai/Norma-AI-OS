use super::{body_text, encode};
use agent::{MemoryKind, MemoryScope};
use storage::{DatabasePaths, MemoryStore};

/// 表单编辑能通过生产解析器，保留中文、引号、类别和已有修改信息。
///
/// 数据构造：
///   - 名称包含中文、冒号与双引号；描述为空，验证可选用途。
///   - type=feedback，modified=2026-10-03；正文为两行中文，含 Markdown 分隔线。
///   - 空个人范围只预览 1 条，不实际发布。
///
/// 执行过程：
///   1. 将字段编码为 Markdown，送入 MemoryStore 的真实导入校验。
///   2. 核对解析后的字段，再剥离 frontmatter 验证正文完整。
///
/// 预期结果：
///   - 正断言：字段、类型、修改信息和正文保持原值。
///   - 负断言：预览不发布主题，正文不包含内部 frontmatter。
#[test]
fn memory_form_roundtrips_through_storage_validation() {
    let temp = tempfile::tempdir().expect("应创建临时根");
    let store = MemoryStore::open(&DatabasePaths::isolated_test(temp.path())).expect("应打开存储");
    let snapshot = store
        .snapshot(&MemoryScope::Personal)
        .expect("应读取空范围");
    let title = "交流偏好：\"中文\"";
    let body = "请用简洁中文。\n---\n保留这条正文分隔线。";
    let markdown = encode(
        title,
        "",
        Some(MemoryKind::Feedback),
        Some("2026-10-03"),
        body,
    );
    let entries = store
        .preview_import(&snapshot, &[("preferences".into(), markdown.clone())])
        .expect("表单编码必须通过真实存储校验");
    assert_eq!(entries[0].name, title, "引号或冒号不能改变名称");
    assert_eq!(entries[0].description, "", "可选描述应允许空字符串");
    assert_eq!(
        entries[0].kind,
        Some(MemoryKind::Feedback),
        "纠正不能改变已有类别"
    );
    assert_eq!(
        entries[0].modified.as_deref(),
        Some("2026-10-03"),
        "编辑不能丢失已有修改信息"
    );
    assert_eq!(
        body_text(&markdown),
        body,
        "正文分隔线不能被误识别为 header"
    );
    assert!(
        !body_text(&markdown).contains("type: feedback"),
        "详情不应暴露内部类型字段"
    );
    assert!(
        store
            .snapshot(&MemoryScope::Personal)
            .expect("应重读范围")
            .entries()
            .next()
            .is_none(),
        "预览不应实际发布任何主题"
    );
}

/// Windows 换行的记忆可直接阅读，普通 Markdown 不被误删。
///
/// 数据构造：一条完整 CRLF frontmatter 文档，一条无 frontmatter 的正文。
/// 执行过程：分别剥离 header，核对保留的正文和普通文档。
/// 预期结果：中文正文保留；frontmatter 不在正文；普通 Markdown 保持原值。
#[test]
fn memory_body_handles_crlf_and_plain_documents() {
    let markdown = "---\r\nname: 中文\r\ndescription: 用途\r\n---\r\n正文";
    assert_eq!(body_text(markdown), "正文", "CRLF header 应正确剥离");
    assert!(
        !body_text(markdown).contains("description:"),
        "内部字段不得展示成记忆正文"
    );
    assert_eq!(
        body_text("# 普通记忆\n正文"),
        "# 普通记忆\n正文",
        "无 header 的旧记忆不能丢失内容"
    );
}
