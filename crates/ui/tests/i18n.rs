use ui::i18n::{LanguagePreference, Locale, LocalizedText};

/// 通用分组与基础设置页面使用不同名称，节能分区及页面说明均随语言切换。
///
/// 数据构造：4 个文案键 × 中文和英文 2 种语言，共 8 个独立期望值。
/// 执行过程：用显式 Locale 读取分组、页面、说明和节能分区文案，不修改进程语言。
/// 预期结果：中英文均返回约定名称；页面名称不得与分组相同，中文不得回退为文案键。
#[test]
fn general_group_and_basic_settings_have_distinct_bilingual_labels() {
    use ui::i18n::text_for;
    let cases = [
        ("general.title", "通用", "General"),
        ("general.basic_settings", "基础设置", "Basic Settings"),
        (
            "general.subtitle",
            "设置界面语言与空闲节能偏好",
            "Set your interface language and idle energy preferences",
        ),
        (
            "settings.performance_energy",
            "性能与节能",
            "Performance & Energy",
        ),
    ];
    for (key, chinese, english) in cases {
        assert_eq!(
            text_for(Locale::ZhCn, key),
            chinese,
            "中文文案应符合设置结构：{key}"
        );
        assert_eq!(
            text_for(Locale::En, key),
            english,
            "英文文案应符合设置结构：{key}"
        );
        assert_ne!(
            text_for(Locale::ZhCn, key),
            key,
            "中文文案不得回退为键：{key}"
        );
    }
    for locale in [Locale::ZhCn, Locale::En] {
        assert_ne!(
            text_for(locale, "general.title"),
            text_for(locale, "general.basic_settings"),
            "通用分组与基础设置页面应有不同名称"
        );
    }
}

/// 跟随系统选择首个受支持的语言，显式选择优先于系统偏好。
///
/// 数据构造（含关键数值的推导过程）：
///   首选列表 = [fr-FR, zh-Hant-TW, en-US]，3 项，第一项不受支持。
///   zh-Hant-TW 属于中文，产品当前只提供简体中文。
///
/// 执行过程：
///   1. 用 System 解析列表 → 跳过法语，选择中文。
///   2. 用 English 解析同一列表 → 使用显式英文。
///   3. 用 System 解析只有法语的列表 → 回退英文。
///
/// 预期结果：
///   - 正断言：系统解析为简体中文；显式英文及不支持的语言回退为英文。
///   - 负断言：显式英文不能被系统中文覆盖。
#[test]
fn language_preference_resolves_system_and_explicit_choices() {
    let preferred = ["fr-FR", "zh-Hant-TW", "en-US"];
    assert_eq!(
        LanguagePreference::System.resolve(&preferred),
        Locale::ZhCn,
        "跟随系统应跳过不支持的法语并选择中文"
    );
    assert_eq!(
        LanguagePreference::English.resolve(&preferred),
        Locale::En,
        "显式英文必须优先于系统中文"
    );
    assert_ne!(
        LanguagePreference::English.resolve(&preferred),
        Locale::ZhCn,
        "系统中文不能覆盖用户的英文选择"
    );
    assert_eq!(
        LanguagePreference::System.resolve(&["fr-FR"]),
        Locale::En,
        "没有支持的系统语言时应回退英文"
    );
}

/// 缓存的产品消息随展示语言更新，用户参数保持原文且不递归替换。
///
/// 数据构造（含关键数值的推导过程）：
///   path = "项目/%{reason}"，包含中文与一个形似占位符的用户值。
///   消息包含 path 与 reason 两个命名参数；同一个消息用于两种语言。
///
/// 执行过程：
///   1. 构造保存失败消息，保留文案键与原始参数。
///   2. 分别用中文和英文解析同一消息。
///
/// 预期结果：
///   - 正断言：两种语言显示各自的完整提示，路径和原因保持原文。
///   - 负断言：路径中的 %{reason} 不能被错误地二次展开。
#[test]
fn cached_message_resolves_again_without_rewriting_user_arguments() {
    let message = LocalizedText::new("general.save_failed")
        .with_arg("path", "项目/%{reason}")
        .with_arg("reason", "EACCES");
    assert_eq!(
        message.resolve_for(Locale::ZhCn),
        "无法保存语言设置（项目/%{reason}）：EACCES",
        "中文提示应保留原始路径和技术诊断"
    );
    assert_eq!(
        message.resolve_for(Locale::En),
        "Could not save language settings (项目/%{reason}): EACCES",
        "缓存消息切换到英文后必须使用英文模板"
    );
    assert!(
        !message.resolve_for(Locale::En).contains("项目/EACCES"),
        "用户参数内的占位符不能被递归展开"
    );
}

/// 已缓存提示通过借用接口在切换语言后更新，普通用户文字不被改写。
///
/// 数据构造：一个无参数保存提示；一条恰好等于产品文案的用户文字。
/// 执行过程：构造两种消息 → 英文读取 → 中文读取 → 恢复英文。
/// 预期结果：保存提示双语更新；用户文字始终为 Saved，不变为“已保存”。
#[test]
fn borrowed_cached_text_tracks_locale_and_keeps_literals() {
    let message = LocalizedText::new("common.save");
    let literal = LocalizedText::from("Saved");
    Locale::En.apply();
    assert_eq!(message.as_str(), "Save", "英文界面必须显示 Save");
    Locale::ZhCn.apply();
    assert_eq!(message.as_str(), "保存", "已有状态消息必须随语言切换");
    assert_eq!(literal.as_str(), "Saved", "用户文字不能通过文案匹配被翻译");
    assert_ne!(literal.as_str(), "已保存", "用户内容必须保持原文");
    Locale::En.apply();
}

/// 壁纸类型、运动状态与选中的本机识别按钮均有双语文案。
///
/// 数据构造：图片、视频、静态、动态、默认壁纸、本机选中状态共 6 个产品键，分别解析中文与英文。
/// 执行过程：通过显式 Locale 读取词库，不修改进程语言。
/// 预期结果：中文为图片/视频/静态/动态/✓ 本机；英文保持既有文案；中文不是硬编码英文。
#[test]
fn conditional_product_labels_have_bilingual_translations() {
    use ui::i18n::text_for;
    assert_eq!(
        text_for(Locale::ZhCn, "settings.wallpaper_default"),
        "默认",
        "内置默认壁纸名称应有中文"
    );
    assert_eq!(
        text_for(Locale::En, "settings.wallpaper_default"),
        "Default",
        "默认壁纸应保留英文名称"
    );
    assert_eq!(
        text_for(Locale::ZhCn, "settings.wallpaper_static"),
        "静态",
        "静态状态应有中文"
    );
    assert_eq!(
        text_for(Locale::En, "settings.wallpaper_static"),
        "Static",
        "静态状态应有英文"
    );
    assert_eq!(
        text_for(Locale::ZhCn, "settings.wallpaper_dynamic"),
        "动态",
        "动态状态应有中文"
    );
    assert_eq!(
        text_for(Locale::En, "settings.wallpaper_dynamic"),
        "Dynamic",
        "动态状态应有英文"
    );
    assert_ne!(
        text_for(Locale::ZhCn, "settings.wallpaper_dynamic"),
        "Dynamic",
        "中文动态状态不得残留英文"
    );
    assert_eq!(
        text_for(Locale::ZhCn, "settings.wallpaper_image"),
        "图片",
        "图片标签应本地化"
    );
    assert_eq!(
        text_for(Locale::En, "settings.wallpaper_image"),
        "IMAGE",
        "英文图片标签应保留既有样式"
    );
    assert_eq!(
        text_for(Locale::ZhCn, "settings.wallpaper_video"),
        "视频",
        "视频标签应本地化"
    );
    assert_eq!(
        text_for(Locale::En, "settings.wallpaper_video"),
        "VIDEO",
        "英文视频标签应保留既有样式"
    );
    assert_eq!(
        text_for(Locale::ZhCn, "voice.on_device_selected"),
        "✓ 本机",
        "选中的本机按钮应有中文"
    );
    assert_eq!(
        text_for(Locale::En, "voice.on_device_selected"),
        "✓ On-device",
        "英文选中按钮应保留勾号"
    );
    assert_ne!(
        text_for(Locale::ZhCn, "voice.on_device_selected"),
        "✓ On-device",
        "中文界面不能残留英文按钮"
    );
}

/// 英文计数使用不依赖数量的标签，避免一个项目或工作空间使用错误复数。
///
/// 数据构造：count 与 project_count 都为 1，表示仅创建一个验收工作空间及项目。
/// 执行过程：用同一个带命名参数的消息分别读取英文和中文。
/// 预期结果：英文显示 Workspaces: 1 和 Projects: 1；中文保持数量单位；英文不出现 1 workspaces。
#[test]
fn workspace_counts_do_not_use_plural_nouns_after_one() {
    let workspaces = LocalizedText::new("workspace.count_workspaces").with_arg("count", 1);
    let projects =
        LocalizedText::new("workspace.project_count_projects_2").with_arg("project_count", 1);
    assert_eq!(
        workspaces.resolve_for(Locale::En),
        "Workspaces: 1",
        "单个工作空间的英文计数必须使用数量中性的标签"
    );
    assert_eq!(
        projects.resolve_for(Locale::En),
        "Projects: 1",
        "单个项目的英文计数必须使用数量中性的标签"
    );
    assert_eq!(
        workspaces.resolve_for(Locale::ZhCn),
        "1 个工作空间",
        "中文计数应保留数量单位"
    );
    assert_eq!(
        projects.resolve_for(Locale::ZhCn),
        "1 个项目",
        "中文项目计数应保留数量单位"
    );
    assert_ne!(
        workspaces.resolve_for(Locale::En),
        "1 workspaces",
        "单个工作空间不能使用错误的英文复数"
    );
}
