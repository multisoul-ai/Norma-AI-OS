use crate::menu_bar::{menu_bar_spec, MenuItemSpec};

/// 原生菜单栏：应用、文件、编辑、布局和帮助命令必须按 macOS 信息架构分组。
///
/// 数据构造（含关键数值的推导过程）：
///   顶级菜单数 = 5：Norma OS / File / Edit / Layout / Help
///   应用菜单动作数 = 3：Check for Updates… + Settings… + Quit Norma OS
///   File 直接项数 = 4：New 子菜单 + separator + Open Project… + Clone GitHub Repository…
///   New 子菜单动作数 = 9：窗口 1 + 卡片 3 + Agent 5
///   New 子菜单 separator 数 = 2：Workspace→Cards、Cards→Agents
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `menu_bar_spec()` 产出与 GPUI 原生 Menu 一一对应的纯描述。
///   2. 读取顶级菜单及 File › New 的项目顺序。
///   3. 分别检查应用级、文件创建、编辑和布局命令的正负边界。
///
/// 预期结果：
///   - 断言 A：应用菜单按系统惯例先提供 Check for Updates…，再提供 Settings…。
///   - 断言 B：File › New 保留窗口、三类卡片和五类 Agent，且两条分隔线表达三组语义。
///   - 断言 C：Copy 只位于 Edit，布局命令只位于 Layout，避免与创建动作混排。
///   - 断言 D：Help 只提供可重复打开的 Feature Guide…，不混入创建或布局动作。
#[test]
fn test_menu_bar_groups_application_file_edit_and_layout_commands() {
    let menus = menu_bar_spec();
    let top_level_names: Vec<_> = menus.iter().map(|menu| menu.label).collect();

    assert_eq!(
        top_level_names,
        vec!["Norma OS", "File", "Edit", "Layout", "Help"],
        "顶级菜单必须按应用、文件、编辑、布局、帮助五组排列，Help 入口不能缺失"
    );
    assert!(
        !top_level_names.contains(&"Window"),
        "当前没有可管理窗口列表时不应渲染空 Window 菜单"
    );

    let app_menu = &menus[0];
    assert_eq!(
        item_labels(&app_menu.items),
        vec![
            "Check for Updates…",
            "Settings…",
            "—",
            "Services",
            "—",
            "Quit Norma OS"
        ],
        "Norma OS 菜单必须把系统熟悉的 Check for Updates… 放在 Settings… 之前"
    );
    assert!(
        !item_labels(&app_menu.items).contains(&"New Terminal"),
        "New Terminal 不应继续混在 Norma OS 应用菜单中"
    );
    assert!(
        !item_labels(&app_menu.items).contains(&"Update Now"),
        "应用菜单不应发明自定义 Update Now 文案或跳过 Sparkle 的标准检查流程"
    );

    let file_menu = &menus[1];
    assert_eq!(
        item_labels(&file_menu.items),
        vec!["New", "—", "Open Project…", "Clone GitHub Repository…",],
        "File 菜单应保留 New 子菜单，并并列提供本地打开与 GitHub 克隆入口"
    );
    let MenuItemSpec::Submenu {
        items: new_items, ..
    } = &file_menu.items[0]
    else {
        panic!("File 的第一项必须是 New 子菜单");
    };
    assert_eq!(
        item_labels(new_items),
        vec![
            "New Window",
            "—",
            "New Terminal",
            "New Browser",
            "Preview File…",
            "—",
            "New Claude Agent",
            "New Codex Agent",
            "New Cursor Agent",
            "New Grok Agent",
            "New OpenCode Agent",
            "New InfCode Agent",
            "New Kimi Agent",
            "New ZCode Agent",
            "New Pi Agent",
            "New Hermes Agent",
        ],
        "New 子菜单必须按 Workspace、Cards、Agents 三组排列，且 Cursor/Grok/OpenCode/InfCode 以及 Kimi/ZCode/Pi/Hermes 入口不能缺失"
    );
    assert!(
        !item_labels(new_items).contains(&"Open Project…"),
        "Open Project… 是 File 级文件入口，不应错误归入创建子菜单"
    );
    assert!(
        !item_labels(new_items).contains(&"Clone GitHub Repository…"),
        "Clone GitHub Repository… 是 File 级项目入口，不应错误归入卡片创建子菜单"
    );

    assert_eq!(
        item_labels(&menus[2].items),
        vec!["Copy"],
        "Copy 应迁移到标准 Edit 菜单，且本次不扩张其他编辑语义"
    );
    assert_eq!(
        item_labels(&menus[3].items),
        vec!["Reflow Layout", "Toggle Master Layout"],
        "布局菜单必须保留现有两种布局操作且顺序稳定"
    );
    assert!(
        !item_labels(&menus[3].items).contains(&"Copy"),
        "Copy 不应出现在 Layout 菜单，避免编辑与空间排布概念混淆"
    );
    assert_eq!(
        item_labels(&menus[4].items),
        vec!["Feature Guide…"],
        "Help 菜单必须提供永久可重复打开的 Feature Guide… 入口"
    );
    assert!(
        !item_labels(&menus[4].items).contains(&"New Terminal")
            && !item_labels(&menus[4].items).contains(&"Reflow Layout"),
        "Help 菜单不应混入卡片创建或画布布局动作"
    );
}

fn item_labels(items: &[MenuItemSpec]) -> Vec<&'static str> {
    items
        .iter()
        .map(|item| match item {
            MenuItemSpec::Action { label, .. }
            | MenuItemSpec::Submenu { label, .. }
            | MenuItemSpec::System { label } => *label,
            MenuItemSpec::Separator => "—",
        })
        .collect()
}
