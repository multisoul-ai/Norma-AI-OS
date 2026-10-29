use ui::i18n::{text_for, Locale, LocalizedText};

/// 场景：左右抽屉的标题、状态、工具及输入提示均支持中英文。
/// 数据构造：从截图中选择六项，分别给出独立的双语期望值。
/// 执行过程：通过公开词库接口读取两种语言，不改变进程语言。
/// 预期结果：文案完整且不回退成键或另一语言。
#[test]
fn drawer_labels_are_bilingual() {
    for (key, zh, en) in [
        ("drawers.needs_attention", "需要你处理", "Needs attention"),
        ("drawers.not_started", "待开始", "Not started"),
        ("drawers.todo", "待办", "To do"),
        ("drawers.project_progress", "项目进度", "Project progress"),
        (
            "drawers.search_address",
            "搜索或输入网址",
            "Search or enter a URL",
        ),
        ("drawers.write_idea", "写下一个想法…", "Write an idea…"),
    ] {
        assert_eq!(text_for(Locale::ZhCn, key), zh, "中文抽屉文案：{key}");
        assert_eq!(text_for(Locale::En, key), en, "英文抽屉文案：{key}");
        assert_ne!(text_for(Locale::En, key), key, "不能显示未翻译的键");
    }
}

/// 场景：后台返回的抽屉操作提示切换语言后仍可重新投影。
/// 数据构造：保存图片失败，原因携带中文路径及看似占位符的原始诊断。
/// 执行过程：构造一次缓存消息，再分别读取中英文。
/// 预期结果：产品前缀随语言变化；路径及诊断不被翻译、也不被二次替换。
#[test]
fn drawer_failure_keeps_arguments_and_switches_language() {
    let error = LocalizedText::new("drawers.image_save_failed")
        .with_arg("error", "草稿/%{error}.png: EACCES");
    assert_eq!(
        error.resolve_for(Locale::En),
        "Could not save image: 草稿/%{error}.png: EACCES",
        "英文错误须保留原始原因"
    );
    assert_eq!(
        error.resolve_for(Locale::ZhCn),
        "没能保存图片：草稿/%{error}.png: EACCES",
        "已有消息应能切回中文"
    );
    assert!(
        !error.resolve_for(Locale::En).starts_with("没能"),
        "英文界面不残留中文产品提示"
    );
}
