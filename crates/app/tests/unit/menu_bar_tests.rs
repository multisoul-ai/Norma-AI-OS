use crate::menu_bar::{menu_bar_spec, MenuItemSpec};

/// 原生菜单栏：应用、文件、编辑和布局命令必须按已确认的 macOS 信息架构分组。
///
/// 数据构造（含关键数值的推导过程）：
///   顶级菜单数 = 4：Norma OS / File / Edit / Layout
///   File 直接项数 = 3：New 子菜单 + separator + Open Project…
///   New 子菜单动作数 = 9：窗口 1 + 卡片 3 + Agent 5
///   New 子菜单 separator 数 = 2：Workspace→Cards、Cards→Agents
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `menu_bar_spec()` 产出与 GPUI 原生 Menu 一一对应的纯描述。
///   2. 读取顶级菜单及 File › New 的项目顺序。
///   3. 分别检查应用级、文件创建、编辑和布局命令的正负边界。
///
/// 预期结果：
///   - 断言 A：顶级菜单严格为 Norma OS、File、Edit、Layout，不再只有扁平 Norma OS。
///   - 断言 B：File › New 保留窗口、三类卡片和五类 Agent，且两条分隔线表达三组语义。
///   - 断言 C：Copy 只位于 Edit，布局命令只位于 Layout，避免与创建动作混排。
///   - 断言 D：旧的顶级 New Terminal、Copy、Settings 不应继续出现在 Norma OS。
#[test]
fn test_menu_bar_groups_application_file_edit_and_layout_commands() {
    let menus = menu_bar_spec();
    let top_level_names: Vec<_> = menus.iter().map(|menu| menu.label).collect();

    assert_eq!(
        top_level_names,
        vec!["Norma OS", "File", "Edit", "Layout"],
        "顶级菜单必须按应用、文件、编辑、布局四组排列，不能退回单一扁平 Norma OS 菜单"
    );
    assert!(
        !top_level_names.contains(&"Window"),
        "当前没有可管理窗口列表时不应渲染空 Window 菜单"
    );

    let app_menu = &menus[0];
    assert_eq!(
        item_labels(&app_menu.items),
        vec!["Settings…", "—", "Services", "—", "Quit Norma OS"],
        "Norma OS 菜单应只承载应用级设置、系统 Services 与退出命令"
    );
    assert!(
        !item_labels(&app_menu.items).contains(&"New Terminal"),
        "New Terminal 不应继续混在 Norma OS 应用菜单中"
    );

    let file_menu = &menus[1];
    assert_eq!(
        item_labels(&file_menu.items),
        vec!["New", "—", "Open Project…"],
        "File 菜单应只保留 New 子菜单和项目打开入口"
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
        ],
        "New 子菜单必须按 Workspace、Cards、Agents 三组排列，且 Cursor/Grok/OpenCode 入口不能缺失"
    );
    assert!(
        !item_labels(new_items).contains(&"Open Project…"),
        "Open Project… 是 File 级文件入口，不应错误归入创建子菜单"
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
