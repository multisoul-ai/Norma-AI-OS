use super::*;
use ui::i18n::Locale;

/// 原生菜单使用指定语言显示，动作类型和产品品牌保持一致。
///
/// 数据构造（含关键数值的推导过程）：
///   两棵菜单树分别使用 ZhCn 与 En；每棵树含 5 个顶级菜单。
///
/// 执行过程：
///   1. 为两种语言构造相同产品菜单。
///   2. 检查文件菜单和设置动作的显示文案。
///
/// 预期结果：
///   - 正断言：中文文件菜单显示“文件”，英文显示“File”。
///   - 正断言：两种语言的设置项都仍绑定 ToggleSettings。
///   - 负断言：中文菜单不能保留英文“File”。
#[test]
fn native_menu_translates_labels_without_changing_actions() {
    let chinese = menu_bar_spec_for(Locale::ZhCn);
    let english = menu_bar_spec_for(Locale::En);
    assert_eq!(chinese[1].label, "文件", "中文界面的原生文件菜单必须本地化");
    assert_eq!(english[1].label, "File", "英文界面的文件菜单必须显示英文");
    assert_ne!(chinese[1].label, "File", "中文菜单不能残留英文标签");
    assert!(
        matches!(
            chinese[0].items[1],
            MenuItemSpec::Action {
                action: MenuAction::ToggleSettings,
                label: "设置…"
            }
        ),
        "中文设置菜单必须保留 ToggleSettings 动作"
    );
    assert!(
        matches!(
            english[0].items[1],
            MenuItemSpec::Action {
                action: MenuAction::ToggleSettings,
                label: "Settings…"
            }
        ),
        "英文设置菜单必须保留 ToggleSettings 动作"
    );
}
