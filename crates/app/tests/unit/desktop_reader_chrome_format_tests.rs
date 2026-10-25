use storage::DesktopContentKind;

/// 浏览器上下文标题应写成原型里的「主机名 · 页面名」，并去掉 www。
///
/// 数据构造：资源 https://www.apple.com/apple-intelligence/，标题 Apple Intelligence。
/// 执行过程：调用内容标题函数，并对照同主机名、文件和来源已移除三种标签。
/// 预期结果：网页标题是 apple.com · Apple Intelligence；主机名与标题相同时不重复；
/// 文件用文件名；来源移除时追加说明。
#[test]
fn browser_context_title_matches_prototype_host_and_page() {
    let web = storage::DesktopContentTab {
        instance_id: String::new(),
        preview_type: None,
        id: "apple".into(),
        workspace_id: "space".into(),
        source_area_id: None,
        source_project_name: "Norma".into(),
        kind: DesktopContentKind::Browser,
        resource: "https://www.apple.com/apple-intelligence/".into(),
        version: None,
        title: "Apple Intelligence".into(),
        position: 0,
        selected: true,
        source_removed: false,
    };
    assert_eq!(
        super::chrome::content_context_title(&web),
        "apple.com · Apple Intelligence",
        "应去掉 www，并用中点连接主机名和页面标题"
    );
    let same = storage::DesktopContentTab {
        title: "apple.com".into(),
        resource: "https://apple.com".into(),
        ..web.clone()
    };
    assert_eq!(
        super::chrome::content_context_title(&same),
        "apple.com",
        "主机名与标题相同时不应写成 apple.com · apple.com"
    );
    let file = storage::DesktopContentTab {
        kind: DesktopContentKind::File,
        resource: "/tmp/notes.md".into(),
        title: String::new(),
        ..web.clone()
    };
    assert_eq!(
        super::chrome::content_context_title(&file),
        "notes.md",
        "空文件标题应回退到文件名"
    );
    let removed = storage::DesktopContentTab {
        source_removed: true,
        ..web
    };
    assert_eq!(
        super::chrome::content_context_title(&removed),
        "apple.com · Apple Intelligence · 来源已移除",
        "来源项目移除后标题必须带说明"
    );
}

/// 右栏标签的静止、悬停、按下和选中色必须能互相区分，浅色和深色主题都一样。
///
/// 数据构造：Paper Blue 与 Sapphire Night 两套主题 token。
/// 执行过程：读取阅读栏颜色，比较透明度字节。
/// 预期结果：悬停比静止更实，按下比悬停更实，选中不同于未选中。
#[test]
fn reader_chrome_colors_separate_rest_hover_pressed_and_selected() {
    for theme in [ui::Theme::paper_blue(), ui::Theme::sapphire_night()] {
        let colors = super::chrome::reader_chrome_colors(theme);
        let alpha = |color: u32| (color & 0xff) as u8;
        assert!(
            alpha(colors.tab_hover) > alpha(colors.tab_rest),
            "未选中标签的悬停透明度必须高于静止"
        );
        assert!(
            alpha(colors.tab_pressed) > alpha(colors.tab_hover),
            "未选中标签的按下透明度必须高于悬停"
        );
        assert!(
            alpha(colors.tab_selected_hover) > alpha(colors.tab_selected),
            "选中标签的悬停必须不同于选中静止"
        );
        assert!(
            alpha(colors.tab_selected_pressed) > alpha(colors.tab_selected_hover),
            "选中标签的按下必须不同于悬停"
        );
        assert_ne!(
            colors.tab_selected, colors.tab_rest,
            "选中填充不能与未选中静止相同"
        );
        assert_ne!(
            colors.tab_selected_border, colors.tab_rest_border,
            "选中边框不能与未选中边框相同"
        );
        assert!(
            alpha(colors.control_hover) > alpha(colors.control_rest),
            "收起按钮悬停必须比静止更明显"
        );
        assert!(
            alpha(colors.control_pressed) > alpha(colors.control_hover),
            "收起按钮按下必须比悬停更明显"
        );
    }
}
