use super::summary_key_activates;

/// 场景：统计卡获得键盘焦点后，回车和空格与点击等价，其余按键不触发。
///
/// 数据构造：无修饰键的 enter、space，带修饰键的 enter，以及 tab、up、escape。
/// 执行过程：逐个询问这个按键是否激活统计卡。
/// 预期结果：
///   - 正断言：无修饰键的 enter 和 space 激活。
///   - 负断言：带修饰键的 enter（⌘↩ 等应用级快捷键）、tab（交给焦点移动）、up（交给概览行导航）、escape（交给收起概览）都不激活。
#[test]
fn summary_cards_activate_on_enter_and_space_only() {
    assert!(summary_key_activates("enter", false));
    assert!(summary_key_activates("space", false));
    assert!(
        !summary_key_activates("enter", true),
        "带修饰键的回车留给快捷键"
    );
    assert!(!summary_key_activates("tab", false), "tab 用来移动焦点");
    assert!(!summary_key_activates("up", false), "方向键留给概览行导航");
    assert!(
        !summary_key_activates("escape", false),
        "escape 留给收起概览"
    );
}
