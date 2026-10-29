#[path = "../support/drawer_locale.rs"]
mod drawer_locale;
use drawer_locale::expected_copy;

use storage::DesktopContentKind;

/// 场景：持久化的系统标签重新展示时随语言变化，文件/网页的用户标题保持原文。
/// 数据构造：同一旧中文标签分别作为文件工作台、空白浏览器和真实文件标题。
/// 执行过程：切换中英文后读取标签标题，最后恢复测试前语言。
/// 预期结果：系统标签翻译；名为“文件”的真实文件不会因为碰巧同名而被翻译。
#[test]
fn drawer_i18n_persisted_system_tab_titles_follow_locale() {
    let previous = ui::i18n::Locale::current();
    let mut tab = storage::DesktopContentTab {
        instance_id: String::new(),
        preview_type: None,
        id: "test".into(),
        workspace_id: "workspace".into(),
        source_area_id: None,
        source_project_name: "用户项目".into(),
        kind: DesktopContentKind::ProjectFiles,
        resource: "project-files".into(),
        version: None,
        title: "文件".into(),
        position: 0,
        selected: true,
        source_removed: false,
    };
    for (locale, files, browser) in [
        (ui::i18n::Locale::En, "Files", "Browser"),
        (ui::i18n::Locale::ZhCn, "文件", "浏览器"),
    ] {
        locale.apply();
        tab.kind = DesktopContentKind::ProjectFiles;
        assert_eq!(
            super::chrome_format::content_tab_title(&tab),
            files,
            "工作台标题由类型决定"
        );
        tab.kind = DesktopContentKind::Browser;
        tab.resource = "about:blank".into();
        assert_eq!(
            super::chrome_format::content_tab_title(&tab),
            browser,
            "空白页标签随语言切换"
        );
        tab.kind = DesktopContentKind::File;
        tab.resource = "/tmp/文件".into();
        assert_eq!(
            super::chrome_format::content_tab_title(&tab),
            "文件",
            "用户标题始终保持原文"
        );
        assert_ne!(
            super::chrome_format::content_tab_title(&tab),
            "Files",
            "不能根据文字匹配误译文件标题"
        );
    }
    previous.apply();
}

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
        expected_copy(
            "apple.com · Apple Intelligence · 来源已移除",
            "apple.com · Apple Intelligence · Source removed"
        ),
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
