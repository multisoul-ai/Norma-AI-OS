use super::cheat_sheet::GROUPS;
use ui::i18n::{text_for, Locale};

/// FG-CHEAT-1：速查表覆盖工作空间、项目、创建卡片、派发想法四组，且每条文案两种语言都有翻译。
///
/// 数据构造（含关键数值的推导过程）：
///   groups = 工作空间(1) + 项目(1) + 创建卡片(1) + 派发想法(1) = 4
///   每组至少 1 行；键位标签非空
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 GROUPS，逐行检查键位标签。
///   2. 对 zh-CN 与 en 解析组标题和每行说明，翻译缺失时 text_for 会回退为 key 本身。
///
/// 预期结果：
///   - 正断言：恰好 4 组，每组非空，键位标签非空。
///   - 负断言：任何文案解析结果都不能等于其 key（缺翻译）。
#[test]
fn cheat_sheet_groups_are_complete_and_localized() {
    assert_eq!(GROUPS.len(), 4, "速查表必须恰好包含四组");
    for group in GROUPS.iter() {
        assert!(!group.rows.is_empty(), "{} 组不能为空", group.title_key);
        for locale in [Locale::ZhCn, Locale::En] {
            assert_ne!(text_for(locale, group.title_key), group.title_key);
            for row in group.rows {
                assert!(!row.keys.is_empty(), "{} 缺少键位", row.label_key);
                assert_ne!(
                    text_for(locale, row.label_key),
                    row.label_key,
                    "{:?} 缺少 {} 的翻译",
                    locale,
                    row.label_key
                );
            }
        }
    }
}
